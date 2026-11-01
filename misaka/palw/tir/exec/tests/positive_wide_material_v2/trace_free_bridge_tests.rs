//! No reference interpreter or prior dense/step binding in this startup path.
//! Only synthetic independently declared model/job, ordinary Miner output, one
//! Panel run, then complete independent Node checking. No native authority.
use super::*;
use kaspa_consensus_core::{
    palw_native_resource_v1::require_source_program_local_profile_v1,
    palw_positive_check_v2::full_relation::wide_v2::stream::StreamSubmitV2,
};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::owned_artifact_params_v2::forward::{metadata::*, *},
};
use std::{
    fs::File,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};
const EXTERNAL: u64 = 1 << 28;
#[derive(Default)]
struct Ledger {
    live: AtomicU64,
    runs: AtomicUsize,
    roles: Mutex<Vec<&'static str>>,
}
struct Guard(Arc<Ledger>, u64);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.live.fetch_sub(self.1, Ordering::SeqCst);
    }
}
fn ledger(l: Arc<Ledger>) -> TirMemoryReserverV1 {
    Arc::new(move |role, _, _, bytes| {
        assert!(bytes > 0);
        if role == "tir-wide-q24-v2-required" {
            l.runs.fetch_add(1, Ordering::SeqCst);
        }
        l.roles.lock().unwrap().push(role);
        l.live.fetch_add(bytes, Ordering::SeqCst);
        Ok(Box::new(Guard(l.clone(), bytes)))
    })
}
fn metadata_limits() -> MetadataOwnerLimitsV2 {
    let l = super::loaded_bridge_tests::startup_limits();
    MetadataOwnerLimitsV2 {
        header: l.header,
        source: l.source,
        compile: l.compile,
        max_new_owner_bytes: l.max_new_owner_bytes,
        max_operation_bytes: l.max_operation_bytes,
        max_attempts: 4,
        max_retained_requests: l.max_new_owner_bytes * 4,
        max_total_read_bytes: l.max_total_read_bytes,
        max_total_work: l.max_total_work,
        max_total_scan_steps: l.max_total_source_steps,
        max_io_calls_per_attempt: 1 << 20,
        max_total_io_calls: 1 << 24,
    }
}
fn load_limits() -> LocalForwardLoadLimitsV2 {
    let l = super::loaded_bridge_tests::startup_limits();
    let mut params = l.params;
    params.max_attempts = 4;
    LocalForwardLoadLimitsV2 {
        params,
        max_new_owner_bytes: 1 << 24,
        max_operation_bytes: 1 << 34,
        max_retained_requests: 1 << 28,
        max_provider_calls_per_attempt: 1 << 18,
        max_total_provider_calls: 1 << 20,
    }
}
fn ordinary_limits(external: u64) -> LocalWideForwardLimitsV2 {
    LocalWideForwardLimitsV2 {
        max_positions: MAX_CONTEXT,
        max_generated: 16,
        max_logits_lanes: 64,
        max_work_per_attempt: 1 << 44,
        max_requested_bytes: 1 << 30,
        max_operation_bytes: 1 << 36,
        max_attempts: 2,
        max_total_work: 1 << 46,
        max_retained_requests: 1 << 32,
        external_retained_bytes: external,
    }
}
struct ColdFixture<'a> {
    program: &'a misaka_palw_tir::TirProgramV1,
    params: &'a TirParams<'a>,
    class: PalwTirClassV1,
    artifact: Hash64,
    job: PalwJobContextV2,
    path: std::path::PathBuf,
}
impl OracleFixture for ColdFixture<'_> {
    fn class(&self) -> &PalwTirClassV1 {
        &self.class
    }
    fn artifact(&self) -> Hash64 {
        self.artifact
    }
    fn job(&self) -> &PalwJobContextV2 {
        &self.job
    }
}
impl ColdFixture<'_> {
    fn file(&self) -> File {
        File::open(&self.path).unwrap()
    }
    fn inputs<'a>(&'a self, ticket: &'a (dyn Send + Sync)) -> MetadataOwnerInputsV2<'a> {
        MetadataOwnerInputsV2 {
            expected_class: self.job.shape_profile_id,
            artifact: self.artifact,
            registered_tokenizer: self.class.tokenizer_id,
            registered_max_context: MAX_CONTEXT,
            original_context: &self.job,
            expected_execution: self.job.context_hash(),
            expected_layout: None,
            external_retained_bytes: EXTERNAL,
            external_ticket: ticket,
        }
    }
}
fn with_cold(window: u32, wide: bool, body: impl FnOnce(&ColdFixture<'_>)) {
    require_source_program_local_profile_v1().expect("audited Source profile required, no skipped bridge tests");
    let (program, map) = run_wide_common::program(wide, window);
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let (class, job) = run_wide_common::declaration(&program, artifact, window);
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("trace-free.palwtir");
    misaka_palw_tir_artifact::write_container_v1(
        &path,
        &program,
        borsh::to_vec(&class.layout).unwrap(),
        class.tokenizer_id.as_bytes(),
        "local-trace-free-panel".into(),
        &mut |p, layer| Ok(params.get(p, layer).unwrap().to_le_bytes()),
    )
    .unwrap();
    // Inventory/serialization only: no interpreter, logits, trace or step oracle.
    body(&ColdFixture { program: &program, params: &params, class, artifact, job, path });
}

#[test]
fn ordinary_miner_then_trace_free_one_pass_panel_reaches_complete_cold_node() {
    for window in [1, 3] {
        with_cold(window, true, |f| {
            let counts = Arc::new(Ledger::default());
            let reserve = ledger(counts.clone());
            let ticket = reserve("test-original-and-export-owners", f.job.shape_profile_id, f.job.context_hash(), EXTERNAL).unwrap();
            let mut meta_budget = MetadataOwnerBudgetV2::new(metadata_limits()).unwrap();
            let mut owner = load_local_forward_metadata_v2(f.file(), f.inputs(&ticket), &mut meta_budget, &reserve).unwrap();
            owner
                .with_inputs(&mut meta_budget, |file, inputs| {
                    let mut load_budget = LocalForwardLoadBudgetV2::new(load_limits()).unwrap();
                    let loaded = load_local_wide_forward_v2(file, inputs, &mut load_budget, &reserve).unwrap();
                    let floor = loaded.load_quote().operation_retained_bytes;
                    assert_eq!(floor, inputs.external_retained_bytes + load_budget.usage().retained_requests);
                    let ordinary = loaded
                        .forward_local(PROMPT, &mut LocalWideForwardBudgetV2::new(ordinary_limits(floor)).unwrap(), &reserve)
                        .unwrap();
                    assert_eq!(ordinary.generated(), EXPECTED);
                    assert_eq!(counts.runs.load(Ordering::SeqCst), 0, "Miner made no dense/witness run");
                    assert!(!counts.roles.lock().unwrap().contains(&"positive-wide-panel-capture-v2"));
                    let signed = SignedMiner::new(f, false);
                    assert_eq!(signed.result.output, digest(ordinary.output_root()));
                    drop(ordinary);
                    let work = loaded.wide_panel_work(bounds(), &disk_reserve()).unwrap();
                    let shared = work.clone();
                    let capture = loaded
                        .capture_wide_panel_one_pass(PROMPT, run_bounds(), &work, tempfile::tempfile().unwrap(), &reserve)
                        .unwrap();
                    assert_eq!(capture.run().generated(), EXPECTED);
                    assert_eq!(counts.runs.load(Ordering::SeqCst), 1);
                    let spent = work.meter();
                    assert!(
                        loaded
                            .capture_wide_panel_one_pass(PROMPT, run_bounds(), &shared, tempfile::tempfile().unwrap(), &reserve)
                            .is_err()
                    );
                    assert_eq!(work.meter(), spent);
                    let result = signed.authenticate();
                    let prep = signed.preparation(f);
                    // Source authenticity remains independent; this is not a raw
                    // parameter getter from the new resident owner.
                    let source = TirParamsSourceV1(f.params);
                    let inventory = TirHeldInventoryV1::build(f.program, &source).unwrap();
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
                            super::loaded_bridge_tests::record_limits(),
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
                    let node_reserve = |bytes| {
                        reserve("test-cold-node", f.job.shape_profile_id, f.job.context_hash(), bytes)
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
                            super::loaded_bridge_tests::record_limits(),
                            &node_reserve,
                        )
                        .unwrap();
                    let relations = prep.plan().summary().relations;
                    let mut frontier = 0;
                    for i in 0..relations {
                        let relation = prep.plan().relation_at(i).unwrap();
                        let material = builder.material(i, frontier).unwrap();
                        assert_eq!(
                            node.submit(&material.as_borrowed(), material.target()).unwrap(),
                            if i + 1 == relations { StreamSubmitV2::NeedEof } else { StreamSubmitV2::Continue }
                        );
                        assert_eq!(node.checked_relations(), i + 1);
                        if matches!(relation.kind, ExpandedRelationKind::Legacy(_)) {
                            frontier += 1;
                        }
                    }
                    assert_eq!(node.finish().unwrap(), CheckOutcome::Matched);
                    assert_eq!(counts.runs.load(Ordering::SeqCst), 1, "Node did no model execution");
                })
                .unwrap();
            drop(owner);
            drop(ticket);
            assert_eq!(counts.live.load(Ordering::SeqCst), 0, "all actual role guards released");
        });
    }
}

#[test]
fn trace_free_bridge_protects_whole_prior_load_floor_and_original_scope() {
    with_cold(1, true, |f| {
        let reserve = ledger(Arc::new(Ledger::default()));
        let ticket = reserve("test-original", f.job.shape_profile_id, f.job.context_hash(), EXTERNAL).unwrap();
        let mut mb = MetadataOwnerBudgetV2::new(metadata_limits()).unwrap();
        let mut owner = load_local_forward_metadata_v2(f.file(), f.inputs(&ticket), &mut mb, &reserve).unwrap();
        owner
            .with_inputs(&mut mb, |file, inputs| {
                let mut lb = LocalForwardLoadBudgetV2::new(load_limits()).unwrap();
                let deny: TirMemoryReserverV1 = Arc::new(|_, _, _, _| Err("denied before payload".into()));
                assert_eq!(load_local_wide_forward_v2(file, inputs, &mut lb, &deny).err(), Some(LocalForwardErrorV2::Guard));
                let prior = lb.usage().retained_requests;
                assert!(prior > 0);
                let loaded = load_local_wide_forward_v2(file, inputs, &mut lb, &reserve).unwrap();
                let floor = loaded.load_quote().operation_retained_bytes;
                assert_eq!(floor, inputs.external_retained_bytes + prior + loaded.load_quote().requested_bytes);
                let mut low = bounds();
                low.max_live_heap_bytes = floor - 1;
                low.external_retained_bytes = 0;
                let disk_calls = Arc::new(AtomicUsize::new(0));
                let seen = disk_calls.clone();
                let disk: WidePanelDiskReserverV2 = Arc::new(move |_, _, _| {
                    seen.fetch_add(1, Ordering::SeqCst);
                    Ok(Box::new(()))
                });
                assert_eq!(loaded.wide_panel_work(low, &disk).err().unwrap(), LocalForwardErrorV2::Limit);
                assert_eq!(disk_calls.load(Ordering::SeqCst), 0);
                let low_work = WidePanelWorkV2::new(
                    {
                        let mut b = bounds();
                        b.external_retained_bytes = floor - 1;
                        b
                    },
                    f.job.shape_profile_id,
                    f.job.context_hash(),
                    &disk_reserve(),
                )
                .unwrap();
                assert!(
                    loaded
                        .capture_wide_panel_one_pass(PROMPT, run_bounds(), &low_work, tempfile::tempfile().unwrap(), &reserve)
                        .is_err()
                );
                assert_eq!(low_work.meter(), Default::default());
                let foreign =
                    WidePanelWorkV2::new(bounds(), f.job.shape_profile_id, Hash64::from_u64_word(999), &disk_reserve()).unwrap();
                assert!(
                    loaded
                        .capture_wide_panel_one_pass(PROMPT, run_bounds(), &foreign, tempfile::tempfile().unwrap(), &reserve)
                        .is_err()
                );
                assert_eq!(foreign.meter(), Default::default());
                let mut ordinary = LocalWideForwardBudgetV2::new(ordinary_limits(floor - 1)).unwrap();
                assert_eq!(loaded.forward_local(PROMPT, &mut ordinary, &reserve).err(), Some(LocalForwardErrorV2::Limit));
                let work = loaded.wide_panel_work(bounds(), &disk_reserve()).unwrap();
                // Existing collector guard denial consumes the shared one-use attempt.
                assert!(
                    loaded.capture_wide_panel_one_pass(PROMPT, run_bounds(), &work, tempfile::tempfile().unwrap(), &deny).is_err()
                );
                let spent = work.meter();
                assert!(spent.visits > 0);
                assert!(
                    loaded.capture_wide_panel_one_pass(PROMPT, run_bounds(), &work, tempfile::tempfile().unwrap(), &reserve).is_err()
                );
                assert_eq!(work.meter(), spent);
            })
            .unwrap();
    });
}

#[test]
fn trace_free_narrow_profile_cannot_enter_the_wide_capture_bridge() {
    with_cold(1, false, |f| {
        let reserve = reserve();
        let ticket = ();
        let mut mb = MetadataOwnerBudgetV2::new(metadata_limits()).unwrap();
        let mut owner = load_local_forward_metadata_v2(f.file(), f.inputs(&ticket), &mut mb, &reserve).unwrap();
        owner
            .with_inputs(&mut mb, |file, inputs| {
                let loaded =
                    load_local_wide_forward_v2(file, inputs, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), &reserve)
                        .unwrap();
                assert_eq!(loaded.wide_panel_work(bounds(), &disk_reserve()).err().unwrap(), LocalForwardErrorV2::UnsupportedProfile);
            })
            .unwrap();
    });
}
