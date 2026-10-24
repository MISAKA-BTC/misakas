//! Actual worker/book handoff controls. Synthetic native rows use checked
//! public restoration/factories; no Q36 weights or live store campaign.
use super::*;
use kaspa_consensus_core::{
    config::params::ForkActivation, palw_attempt_v2::attempt_id_v2, palw_backend::*, palw_reader_router_v2::*, palw_state_v2::*,
    palw_step::*,
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn native_fp_owner_fixture_v2() -> (Arc<Owner>, Arc<Owner>, Arc<Owner>) {
    let mut profile =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .unwrap();
    profile.layer_count = 1;
    profile.full_attention_interval = 0;
    let node = |desc: &str, width, refs: &[u16], name: &str| PalwStepNodeV1 {
        op_kind: PalwStepOpKindV1::MatMulQuant,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: name.into(),
        weight_dtypes: vec![],
        out_len: PalwStepOutLenV1::Fixed { elements: width },
        tile_len: 4,
        kernel_semantics_id: kaspa_consensus_core::palw_step::kernel_semantics_id_v1(desc),
        input_refs: refs.to_vec(),
    };
    use kaspa_consensus_core::palw_step_refute::{
        KDESC_Q36_MATMUL_GROUPED, KDESC_Q36_MOE_COMBINE, KDESC_Q36_ROUTER_TOPK, KDESC_Q36_SIGMOID,
    };
    profile.gdn_nodes = vec![
        node(KDESC_Q36_SIGMOID, 16, &[PALW_STEP_INPUT_LAYER_IN], "score"),
        node(KDESC_Q36_ROUTER_TOPK, 4, &[0], "router"),
        node(KDESC_Q36_MATMUL_GROUPED, 16, &[0], "test.routed"),
        node(KDESC_Q36_MOE_COMBINE, 8, &[2, 1], "combine"),
    ];
    let class = profile.shape_profile_id();
    let artifact = h(77);
    let bond = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(5), 0));
    let state_params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, class, 4, 1000, 100, 1000, 0).unwrap();
    let context = |block, daa| PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: daa, subsidy: 0 };
    let objects = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: class,
            artifact_root: artifact,
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        PalwConsensusObjectV2::BondRegistered {
            bond,
            pubkey: vec![7],
            operator_pubkey: vec![8],
            collateral: 1000,
            payout_payload: h(9),
            capable_classes: Default::default(),
            signature: vec![],
        },
    ];
    let (state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params, &context(1, 1), &objects, None).unwrap();
    let mut envelope = crate::palw_rider_outbox::rider_fixture_v2(h(0), 0);
    envelope.attempt.version = kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION;
    envelope.attempt.class_id = class;
    envelope.attempt.artifact_root = artifact;
    envelope.attempt.executor_bond = bond.0;
    envelope.attempt.operator_id = palw_operator_id_v2(&[8]);
    envelope.attempt.trace_retention_daa = 100;
    let lead = attempt_id_v2(&envelope.attempt);
    let (state, _) = apply_palw_transition_v2(&state, &state_params, &context(2, 10), &[], Some(&envelope)).unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(&state);
    carriage.classes.get_mut(&class).unwrap().registrant_bond = Some(bond);
    carriage.fp_work_profiles.insert(class, Box::new(profile.clone()));
    let mut params = kaspa_consensus_core::config::params::devnet_shipped_params();
    params.palw_offence_attribution = None;
    params.palw_memory_routing_court_v2 = Some(ForkActivation::new(1));
    params.palw_capacity_multi_claim = Some(ForkActivation::new(1));
    let net = b"rider-owner-test";
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(net, Some(params.genesis.hash));
    let mut route = kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(domain, h(3)),
        None,
        Default::default(),
    );
    let base = profile.pre_nodes.len() as u32;
    let row = PalwReaderRouterRowV2 {
        declaration: PalwReaderRouterDeclarationV2 {
            version: 2,
            class_id: class,
            artifact_root: artifact,
            bindings: vec![
                PalwReaderRouterBindingV2 { reader_slot: base + 2, router_slot: base + 1 },
                PalwReaderRouterBindingV2 { reader_slot: base + 3, router_slot: base + 1 },
            ],
        },
        registrant: bond,
        bound_from_daa: 1,
    };
    route.aux.insert((PALW_READER_ROUTER_TABLE_V2, class.as_byte_slice().to_vec()), borsh::to_vec(&row).unwrap());

    let mut job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 1, 1);
    job.network_id = net.to_vec();
    job.trace_scheme_id = kaspa_consensus_core::palw_v2::trace_scheme_id_v2();
    job.job_nullifier = Hash64::default();
    job.assignment_id = Hash64::default();
    let semantics = kaspa_consensus_core::palw_class_semantics_v1::PalwClassSemanticsV1::of_registration(class, artifact, &job, 1);
    carriage.class_semantics.insert(class, semantics.clone());
    carriage.claim_semantics.insert(lead, semantics);
    let claim = carriage.claims.get_mut(&lead).unwrap();
    claim.source = PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() };
    claim.job_identity = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&job);
    carriage.bounded_immature = carriage.bounded_immature.checked_sub(claim.immature_contribution).unwrap();
    claim.immature_contribution = 0;
    let key = |prefix, id: Hash64| {
        let mut k = vec![prefix];
        k.extend_from_slice(id.as_byte_slice());
        k
    };
    route.aux.insert((PALW_READER_ROUTER_TABLE_V2, key(0xA1, class)), borsh::to_vec(&10u64).unwrap());
    route.aux.insert((PALW_READER_ROUTER_TABLE_V2, key(0xA2, lead)), borsh::to_vec(&class).unwrap());
    carriage.kernel_route = Some(route);
    let state = carriage.into_state(&state_params, None).unwrap();
    let source = Arc::new(NativeRetainedV1::try_from_clone_v1(&state).unwrap());
    let first = Owner::from_native_claim_v2(source.clone(), &params, h(2), lead, &job, net, 4).unwrap().unwrap();
    let changed_import = Owner::from_native_claim_v2(source, &params, h(2), lead, &job, net, 5).unwrap().unwrap();
    let (forward, _) = apply_palw_transition_v2(&state, &state_params, &context(3, 11), &[], None).unwrap();
    let forward = Owner::from_native_claim_v2(
        Arc::new(NativeRetainedV1::try_from_clone_v1(&forward).unwrap()),
        &params,
        h(3),
        lead,
        &job,
        net,
        4,
    )
    .unwrap()
    .unwrap();
    (Arc::new(first), Arc::new(forward), Arc::new(changed_import))
}

struct RecordingBackend {
    expected: Arc<Owner>,
    bound: Option<Arc<Owner>>,
    binds: Arc<AtomicUsize>,
    executes: Arc<AtomicUsize>,
    ready: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl PalwExecutionBackendV1 for RecordingBackend {
    fn model_id(&self) -> &str {
        "routing-handoff-refusal-control"
    }
    fn job_for_anchor(&self, _: Hash64) -> Result<(kaspa_consensus_core::palw_v2::PalwJobContextV2, Vec<usize>), String> {
        Err("unused".into())
    }
    fn execute(&self, _: &kaspa_consensus_core::palw_v2::PalwJobContextV2, _: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        Err("unused".into())
    }
    fn verify_material(&self, _: &[u8], _: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        PalwMaterialVerdictV1::Unverifiable
    }
    fn bind_native_routing_claim_v2(
        &mut self,
        owner: Arc<Owner>,
    ) -> Result<(), kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1> {
        assert!(Arc::ptr_eq(&owner, &self.expected));
        self.binds.fetch_add(1, Ordering::SeqCst);
        self.bound = Some(owner);
        Ok(())
    }
    fn execute_free_prompt(
        &self,
        _: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        _: &[usize],
    ) -> Result<PalwFpRunV1, String> {
        assert!(
            self.bound.as_ref().is_some_and(|owner| Arc::ptr_eq(owner, &self.expected)),
            "the exact bound backend instance reaches the worker"
        );
        self.executes.fetch_add(1, Ordering::SeqCst);
        self.ready.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        // No Q36 arithmetic or positive roots are fabricated by this handoff fixture.
        Err("intentional local execution refusal after verified owner handoff".into())
    }
}
async fn completed(replays: &mut PalwSeatReplaysV1, key: &PalwSeatReplayKeyV1) -> PalwSeatReplayPollV1 {
    for _ in 0..2000 {
        let poll = replays.poll(key, 12).await;
        if !matches!(poll, PalwSeatReplayPollV1::Running) {
            return poll;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("worker did not finish");
}
#[tokio::test]
async fn fp_routed_replay_original_native_owner_reaches_worker_detached_cache_and_refresh() {
    let (original, forward, changed_import) = native_fp_owner_fixture_v2();
    assert!(original.is_free_prompt());
    assert!(original.same_claim_v2(&forward));
    assert!(!original.same_claim_v2(&changed_import));
    let binds = Arc::new(AtomicUsize::new(0));
    let executes = Arc::new(AtomicUsize::new(0));
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let backend = Box::new(RecordingBackend {
        expected: original.clone(),
        bound: None,
        binds: binds.clone(),
        executes: executes.clone(),
        ready: ready_tx,
        release: Mutex::new(release_rx),
    });
    let bound = BoundFpReplayV2::bind_with(
        backend,
        original.claim(),
        original.job(),
        true,
        |_, _| Ok(Some(original.clone())),
        BoundFpReplayV2::context_scratch_v2().unwrap(),
    )
    .unwrap();
    let guard = Arc::downgrade(bound.guard.as_ref().unwrap());
    let floor = crate::palw_panel::seat_s_tests::floor_backend();
    let (job, prompt) = crate::palw_panel::seat_s_tests::floor_fp_job(&floor);
    let key = (original.claim(), original.job().job_id);
    let ledger =
        crate::palw_memory_ledger::PalwMemoryLedgerV1::new(crate::palw_memory_ledger::PalwMemoryPoolV1::Host, Some(128), || None);
    let memory = ledger
        .reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 { role: "routing-replay-lifetime-test", class_id: h(1), job: key.0 },
            128,
        )
        .unwrap();
    let mut replays = PalwSeatReplaysV1::default();
    replays.start_fp_routed_v2(key, h(1), false, 10, Some(memory), bound, job, prompt.into_iter().map(|t| t as usize).collect());
    ready_rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    assert_eq!((binds.load(Ordering::SeqCst), executes.load(Ordering::SeqCst)), (1, 1));
    replays.retain_live(|_| false);
    assert!(guard.upgrade().is_some());
    assert_eq!(ledger.reserved_bytes(), 128);
    release_tx.send(()).unwrap();
    assert!(matches!(completed(&mut replays, &key).await, PalwSeatReplayPollV1::Done { result: Err(_), .. }));
    assert!(guard.upgrade().is_some(), "the original guard returns into actual done cache");
    assert_eq!(ledger.reserved_bytes(), 0);
    replays
        .refresh_fp_routed_with_v2(&key, true, |claim, job| {
            assert_eq!(claim, original.claim());
            assert_eq!(job, original.job());
            Ok(Some(forward))
        })
        .unwrap();
    assert!(matches!(
        replays.refresh_fp_routed_with_v2(&key, true, |_, _| Err("native resource/read unavailable".into())),
        Err(FpReplayRoutingFaultV2::Retry(_))
    ));
    assert!(guard.upgrade().is_some(), "native read failures retain original result/owner for retry");
    assert!(matches!(
        replays.refresh_fp_routed_with_v2(&key, true, |_, _| Ok(Some(changed_import))),
        Err(FpReplayRoutingFaultV2::Stale)
    ));
    assert!(guard.upgrade().is_none(), "known import identity change retires only this cached execution");
    assert!(matches!(replays.poll(&key, 13).await, PalwSeatReplayPollV1::Absent));
}
#[tokio::test]
async fn fp_routed_replay_real_floor_execution_retains_original_unrouted_marker() {
    let backend = crate::palw_panel::seat_s_tests::floor_backend();
    let (job, prompt) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
    let prompt: Vec<usize> = prompt.into_iter().map(|t| t as usize).collect();
    let baseline = backend.execute_free_prompt(&job, &prompt).unwrap();
    let context = backend.capture_shape(&baseline.outcome.material).unwrap().job_context;
    let key = (h(70), context.job_id);
    let bound = BoundFpReplayV2::bind_with(
        Box::new(backend),
        key.0,
        &context,
        true,
        |_, _| Ok(None),
        BoundFpReplayV2::context_scratch_v2().unwrap(),
    )
    .unwrap();
    let guard = Arc::downgrade(bound.guard.as_ref().unwrap());
    let mut replays = PalwSeatReplaysV1::default();
    replays.start_fp_routed_v2(key, job.class_id, false, 10, None, bound, job, prompt);
    assert!(matches!(completed(&mut replays, &key).await, PalwSeatReplayPollV1::Done { result: Ok(_), .. }));
    replays
        .refresh_fp_routed_with_v2(&key, true, |claim, job| {
            assert_eq!(claim, key.0);
            assert_eq!(job, &context);
            Ok(None)
        })
        .unwrap();
    assert!(matches!(
        replays.refresh_fp_routed_with_v2(&key, true, |_, _| Err("late native read unavailable".into())),
        Err(FpReplayRoutingFaultV2::Retry(_))
    ));
    assert!(guard.upgrade().is_some());
    let (now_routed, _, _) = native_fp_owner_fixture_v2();
    assert!(matches!(replays.refresh_fp_routed_with_v2(&key, true, |_, _| Ok(Some(now_routed))), Err(FpReplayRoutingFaultV2::Stale)));
    assert!(guard.upgrade().is_none());
    assert!(matches!(replays.poll(&key, 13).await, PalwSeatReplayPollV1::Absent));
}

#[test]
fn fp_routed_replay_capacity_refuses_before_native_read_or_execution() {
    const CHILD: &str = "MISAKA_FP_REPLAY_ROUTING_CAPACITY_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::fp_routed_replay_capacity_refuses_before_native_read_or_execution", module_path!());
        let name = name.split_once("::").unwrap().1;
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let backend = crate::palw_panel::seat_s_tests::floor_backend();
    let (job, _) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
    let context = backend.fp_job_context_v1(&job).unwrap();
    kaspa_consensus_core::palw_native_resource_v1::arm_native_logical_limits_v1(Some(
        kaspa_consensus_core::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 },
    ));
    assert!(BoundFpReplayV2::context_scratch_v2().is_err());
    let result = BoundFpReplayV2::bind_with(
        Box::new(backend),
        h(1),
        &context,
        true,
        |_, _| panic!("native read must not follow refused retained job admission"),
        None,
    );
    assert!(matches!(result, Err(FpReplayRoutingFaultV2::Resource(_))));
}
