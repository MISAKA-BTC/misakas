#![cfg(feature = "node")]
//! Synthetic declarations only. No actual registered model/native receipt,
//! native assignment, trace binding, full leaf/tree execution or model file.
mod run_wide_common;
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{SourceProgramLocalLimitsV1, require_source_program_local_profile_v1},
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_step_refute::{flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1},
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir_artifact::{bounded_v1::PalwTirHeaderLimitsV1, write_container_v1};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirHeldInventoryV1, TirParamsSourceV1,
        memory::TirMemoryReserverV1,
        owned_artifact_params_v2::{
            OwnedParamLimitsV2,
            forward::{metadata::*, *},
        },
    },
    plan::TirPlanCompileLimitsV2,
};
use run_wide_common::{EXPECTED_GENERATED, MAX_CONTEXT, PROMPT};
use std::{
    fs::{File, OpenOptions},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};
const EXTERNAL: u64 = 1 << 30;
struct Guard(Arc<AtomicU64>, u64);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_sub(self.1, Ordering::SeqCst);
    }
}
fn reserve(live: Arc<AtomicU64>, calls: Arc<AtomicUsize>) -> TirMemoryReserverV1 {
    Arc::new(move |_, _, _, bytes| {
        calls.fetch_add(1, Ordering::SeqCst);
        live.fetch_add(bytes, Ordering::SeqCst);
        Ok(Box::new(Guard(live.clone(), bytes)))
    })
}
fn limits() -> MetadataOwnerLimitsV2 {
    MetadataOwnerLimitsV2 {
        header: PalwTirHeaderLimitsV1 {
            max_file_bytes: 1 << 24,
            max_header_bytes: 1 << 20,
            max_program_bytes: 1 << 20,
            max_layout_bytes: 1 << 20,
            max_tensors: 1024,
            max_meta_bytes: 4096,
            max_native_bytes: 1 << 22,
            max_copy_bytes: 1 << 22,
            max_work: 1 << 42,
            max_operation_bytes: 1 << 36,
            external_retained_bytes: 0,
        },
        source: SourceProgramLocalLimitsV1 {
            max_program_bytes: 1 << 20,
            max_scan_steps: 1 << 36,
            max_nf_items: 1 << 32,
            max_native_requests: 1 << 30,
        },
        compile: TirPlanCompileLimitsV2 {
            max_quote_visits: 1 << 36,
            max_retained_bytes: 1 << 30,
            max_scratch_bytes: 1 << 31,
            max_operation_bytes: 1 << 36,
            external_retained_bytes: 0,
        },
        max_new_owner_bytes: 1 << 32,
        max_operation_bytes: 1 << 38,
        max_attempts: 4,
        max_retained_requests: 1 << 36,
        max_total_read_bytes: 1 << 28,
        max_total_work: 1 << 48,
        max_total_scan_steps: 1 << 44,
        max_io_calls_per_attempt: 1 << 20,
        max_total_io_calls: 1 << 24,
    }
}
fn load_limits() -> LocalForwardLoadLimitsV2 {
    LocalForwardLoadLimitsV2 {
        params: OwnedParamLimitsV2 {
            max_tensor_bytes: 1 << 24,
            max_payload_bytes: 1 << 25,
            max_chunk_bytes: 32768,
            max_metadata_bytes: 1 << 24,
            max_native_bytes: 1 << 26,
            max_inventory_leaves: 1 << 20,
            max_work_per_attempt: 1 << 42,
            max_attempts: 4,
            max_total_read_bytes: 1 << 28,
            max_total_inventory_leaves: 1 << 22,
            max_total_work: 1 << 46,
            max_operation_bytes: 1 << 38,
            external_retained_bytes: 0,
        },
        max_new_owner_bytes: 1 << 28,
        max_operation_bytes: 1 << 38,
        max_retained_requests: 1 << 32,
        max_provider_calls_per_attempt: 1 << 20,
        max_total_provider_calls: 1 << 24,
    }
}
fn run_limits(external: u64) -> LocalWideForwardLimitsV2 {
    LocalWideForwardLimitsV2 {
        max_positions: MAX_CONTEXT,
        max_generated: 16,
        max_logits_lanes: 64,
        max_work_per_attempt: 1 << 44,
        max_requested_bytes: 1 << 32,
        max_operation_bytes: 1 << 40,
        max_attempts: 4,
        max_total_work: 1 << 48,
        max_retained_requests: 1 << 35,
        external_retained_bytes: external,
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    class: PalwTirClassV1,
    artifact: Hash64,
    ctx: PalwJobContextV2,
    execution: Hash64,
}
impl Fixture {
    fn new(wide: bool, flat: bool, declared_layout: bool) -> Self {
        // Metadata/provider fixture setup is itself under the independent
        // EXTERNAL ticket in the tests. No LLM forward/evidence run takes place.
        let (mut p, map) = run_wide_common::program(wide, 1);
        if flat {
            p.logits_scheme_id = flat_logits_scheme_id_v1().as_bytes();
        }
        let plan = TirPlan::compile(&p).unwrap();
        let params = TirParams::from_map(&plan, &map).unwrap();
        let source = TirParamsSourceV1(&params);
        let inv = TirHeldInventoryV1::build(&p, &source).unwrap();
        let artifact = inv.tree.root();
        let class = PalwTirClassV1 {
            version: 1,
            program: p.encode(),
            layout: PalwTirLayoutV1 {
                version: 1,
                max_context: MAX_CONTEXT,
                checkpoint_interval: 3,
                h_tile: 1,
                commit_tiles: p.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).map(|_| 4).collect(),
                state_tiles: vec![4; p.states.len()],
            },
            tokenizer_id: Hash64::from_u64_word(18),
        };
        let zero = Hash64::default();
        let ctx = PalwJobContextV2 {
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
            trace_scheme_id: if wide {
                kaspa_consensus_core::palw_logits_q24_v2::scheme_id()
            } else if flat {
                kaspa_consensus_core::palw_v2::trace_scheme_id_v2()
            } else {
                tiled_logits_scheme_id_v1()
            },
            cu_ruleset_id: zero,
            tokenizer_id: zero,
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, PROMPT).unwrap(),
            declared_prefill_tokens: 2,
            exact_decode_tokens: 6,
            max_context_tokens: MAX_CONTEXT,
        };
        let execution = ctx.context_hash();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("metadata.palwtir");
        write_container_v1(
            &path,
            &p,
            if declared_layout { borsh::to_vec(&class.layout).unwrap() } else { vec![] },
            class.tokenizer_id.as_bytes(),
            "test-local-no-trace".into(),
            &mut |j, l| Ok(params.get(j, l).unwrap().to_le_bytes()),
        )
        .unwrap();
        Self { _dir: dir, path, class, artifact, ctx, execution }
    }
    fn file(&self) -> File {
        OpenOptions::new().read(true).write(true).open(&self.path).unwrap()
    }
    fn inputs<'a>(&'a self, ticket: &'a (dyn Send + Sync), layout: bool) -> MetadataOwnerInputsV2<'a> {
        MetadataOwnerInputsV2 {
            expected_class: self.ctx.shape_profile_id,
            artifact: self.artifact,
            registered_tokenizer: self.class.tokenizer_id,
            registered_max_context: MAX_CONTEXT,
            original_context: &self.ctx,
            expected_execution: self.execution,
            expected_layout: layout.then_some(&self.class.layout),
            external_retained_bytes: EXTERNAL,
            external_ticket: ticket,
        }
    }
}
fn audited() {
    require_source_program_local_profile_v1().expect("audited Source build required; unsupported is not a successful metadata test");
}
#[test]
fn trace_free_same_descriptor_owner_reaches_ordinary_outputs_all_three_profiles() {
    audited();
    for (wide, flat, expected) in [
        (true, false, LocalForwardProfileV2::WideI64Q24),
        (false, false, LocalForwardProfileV2::TiledI32),
        (false, true, LocalForwardProfileV2::FlatI32),
    ] {
        let held = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let r = reserve(held.clone(), calls.clone());
        let ticket = r("test-external", Hash64::default(), Hash64::default(), EXTERNAL).unwrap();
        let f = Fixture::new(wide, flat, true);
        let mut b = MetadataOwnerBudgetV2::new(limits()).unwrap();
        let mut owner = load_local_forward_metadata_v2(f.file(), f.inputs(&ticket, false), &mut b, &r).unwrap();
        let q = owner.quote();
        assert_eq!(held.load(Ordering::SeqCst), EXTERNAL + q.actual_guard_allowance);
        assert_eq!(q.source_complete, q.compile.source_owner_requests());
        assert_eq!(q.space_decode, q.source_complete);
        assert_eq!(q.space_revalidate, q.source_complete);
        assert!(
            q.space_peak
                >= 2 * q.source_complete
                    + q.space_layout
                    + q.space_geometry
                    + q.space_fixed.space_root_bytes
                    + q.space_fixed.diagnostic_peak_bytes
        );
        assert_eq!(q.bridge_external_requests, EXTERNAL + q.actual_guard_allowance);
        assert!(q.compile_stage_peak >= q.source_complete + q.space_peak + q.compile.retained_bytes() + q.compile.scratch_bytes());
        let output = owner
            .with_inputs(&mut b, |file, i| {
                assert_eq!(i.class, &f.class);
                assert_eq!(i.original_context, &f.ctx);
                assert_eq!(i.class.tokenizer_id, Hash64::from_u64_word(18));
                assert_eq!(i.original_context.tokenizer_id, Hash64::default());
                assert_eq!(i.original_context.cu_ruleset_id, Hash64::default());
                assert_eq!(i.original_context.max_context_tokens, MAX_CONTEXT);
                let loaded =
                    load_local_wide_forward_v2(file, i, &mut LocalForwardLoadBudgetV2::new(load_limits()).unwrap(), &r).unwrap();
                assert_eq!(loaded.profile(), expected);
                let external = q.bridge_external_requests + loaded.load_quote().requested_bytes;
                loaded.forward_local(PROMPT, &mut LocalWideForwardBudgetV2::new(run_limits(external)).unwrap(), &r).unwrap()
            })
            .unwrap();
        assert_eq!(output.generated(), EXPECTED_GENERATED);
        assert_eq!(output.execution(), f.execution);
        let output_bytes = output.retained_requested_bytes();
        assert_eq!(held.load(Ordering::SeqCst), EXTERNAL + q.actual_guard_allowance + output_bytes);
        drop(owner);
        assert_eq!(held.load(Ordering::SeqCst), EXTERNAL + output_bytes);
        drop(output);
        drop(ticket);
        assert_eq!(held.load(Ordering::SeqCst), 0);
        assert_eq!(calls.load(Ordering::SeqCst), 5);
        assert_eq!(b.usage().borrowed_validations, 1);
    }
}
#[test]
fn required_guard_denial_is_before_backend_and_keeps_attempt_reservation() {
    audited();
    let f = Fixture::new(true, false, true);
    let ticket = ();
    let mut l = limits();
    l.max_attempts = 1;
    let mut b = MetadataOwnerBudgetV2::new(l).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let deny: TirMemoryReserverV1 = Arc::new(move |_, class, execution, n| {
        seen.fetch_add(1, Ordering::SeqCst);
        assert_eq!(n, 1 << 32);
        assert_ne!(class, Hash64::default());
        assert_ne!(execution, Hash64::default());
        Err("test guard refused".into())
    });
    assert_eq!(
        load_local_forward_metadata_v2(f.file(), f.inputs(&ticket, false), &mut b, &deny).err(),
        Some(MetadataOwnerErrorV2::Guard)
    );
    assert_eq!(b.usage().read_bytes, 0);
    assert_eq!(b.source_observations().0, 0);
    let before = b.usage();
    assert_eq!(before.retained_requests, l.max_new_owner_bytes);
    assert_eq!(before.io_calls_reserved, l.max_io_calls_per_attempt);
    assert_eq!(
        load_local_forward_metadata_v2(f.file(), f.inputs(&ticket, false), &mut b, &deny).err(),
        Some(MetadataOwnerErrorV2::Limit)
    );
    assert_eq!(before, b.usage());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn layout_none_requires_exact_independent_preimage_and_never_repairs_declared_layout() {
    audited();
    let ticket = ();
    let r = reserve(Arc::new(AtomicU64::new(0)), Arc::new(AtomicUsize::new(0)));
    let f = Fixture::new(false, false, false);
    let mut b = MetadataOwnerBudgetV2::new(limits()).unwrap();
    assert_eq!(
        load_local_forward_metadata_v2(f.file(), f.inputs(&ticket, false), &mut b, &r).err(),
        Some(MetadataOwnerErrorV2::MissingLayout)
    );
    let spent = b.usage();
    assert!(spent.read_bytes > 0 && spent.work > 0 && spent.retained_requests > 0);
    let mut owner = load_local_forward_metadata_v2(f.file(), f.inputs(&ticket, true), &mut b, &r).unwrap();
    owner.with_inputs(&mut b, |_, i| assert_eq!(i.class.layout, f.class.layout)).unwrap();
    let g = Fixture::new(true, false, true);
    let mut wrong = g.class.layout.clone();
    wrong.checkpoint_interval += 1;
    let mut i = g.inputs(&ticket, true);
    i.expected_layout = Some(&wrong);
    assert_eq!(load_local_forward_metadata_v2(g.file(), i, &mut b, &r).err(), Some(MetadataOwnerErrorV2::Pin));
}
#[test]
fn source_cap_stage_rejects_before_scan_and_failed_io_is_not_free() {
    audited();
    let f = Fixture::new(false, true, true);
    let ticket = ();
    let r = reserve(Arc::new(AtomicU64::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut l = limits();
    l.max_new_owner_bytes = l.source.max_native_requests - 1;
    let mut b = MetadataOwnerBudgetV2::new(l).unwrap();
    assert_eq!(
        load_local_forward_metadata_v2(f.file(), f.inputs(&ticket, false), &mut b, &r).err(),
        Some(MetadataOwnerErrorV2::Limit)
    );
    assert_eq!(b.source_observations().0, 0);
    assert!(b.usage().read_bytes > 0);
    let mut l = limits();
    l.max_io_calls_per_attempt = 1;
    l.max_attempts = 1;
    let mut b = MetadataOwnerBudgetV2::new(l).unwrap();
    assert_eq!(
        load_local_forward_metadata_v2(f.file(), f.inputs(&ticket, false), &mut b, &r).err(),
        Some(MetadataOwnerErrorV2::Header)
    );
    let spent = b.usage();
    assert_eq!(spent.io_calls_reserved, 1);
    assert!(spent.read_bytes > 0);
    assert_eq!(
        load_local_forward_metadata_v2(f.file(), f.inputs(&ticket, false), &mut b, &r).err(),
        Some(MetadataOwnerErrorV2::Limit)
    );
    assert_eq!(spent, b.usage());
}
#[test]
fn independent_class_artifact_context_pins_and_changed_descriptor_fail_closed() {
    audited();
    let f = Fixture::new(true, false, true);
    let ticket = ();
    let held = Arc::new(AtomicU64::new(0));
    let r = reserve(held.clone(), Arc::new(AtomicUsize::new(0)));
    for field in 0..3 {
        let mut i = f.inputs(&ticket, false);
        match field {
            0 => i.artifact = Hash64::from_u64_word(987),
            1 => i.expected_execution = Hash64::from_u64_word(654),
            _ => i.registered_tokenizer = Hash64::from_u64_word(321),
        };
        let mut b = MetadataOwnerBudgetV2::new(limits()).unwrap();
        assert_eq!(load_local_forward_metadata_v2(f.file(), i, &mut b, &r).err(), Some(MetadataOwnerErrorV2::Pin));
        assert_eq!(held.load(Ordering::SeqCst), 0);
        assert!(b.usage().work > 0);
    }
    let mut b = MetadataOwnerBudgetV2::new(limits()).unwrap();
    let mut owner = load_local_forward_metadata_v2(f.file(), f.inputs(&ticket, false), &mut b, &r).unwrap();
    let before = b.usage();
    f.file().set_len(owner.file_stamp().bytes + 1).unwrap();
    assert_eq!(
        owner.with_inputs(&mut b, |_, _| panic!("changed file must not reach borrowed API")).err(),
        Some(MetadataOwnerErrorV2::Changed)
    );
    assert!(b.usage().work > before.work);
    assert_eq!(b.usage().borrowed_validations, before.borrowed_validations + 1);
    drop(owner);
    assert_eq!(held.load(Ordering::SeqCst), 0);
}
#[test]
fn nested_external_double_count_and_zero_limits_are_refused() {
    let mut l = limits();
    l.header.external_retained_bytes = 1;
    assert!(matches!(MetadataOwnerBudgetV2::new(l), Err(MetadataOwnerErrorV2::Pin)));
    let mut l = limits();
    l.compile.external_retained_bytes = 1;
    assert!(matches!(MetadataOwnerBudgetV2::new(l), Err(MetadataOwnerErrorV2::Pin)));
    let mut l = limits();
    l.max_total_io_calls = 0;
    assert!(matches!(MetadataOwnerBudgetV2::new(l), Err(MetadataOwnerErrorV2::Limit)));
}
