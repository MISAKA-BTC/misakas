//! Separate actual-FP source kind, never reinterprets the old material manifest.
//! Child of existing wire module: uses its actual WireNode/Factory/Ctl driver.
//! No target DB/native seed copy, no synthetic Claim/world/aux/test approval.
use super::*;
use std::path::Path;
use kaspa_consensus_core::palw_state_v2::PalwChainStateV2;
use crate::pipeline::virtual_processor::tests::g14_kernel_route_e2e::model_conformance::flat_native_v9::funded11::owning39::master::node_e2e::actual_fp_wire_source_v1::FpWireSourceDescriptorV1;
pub(crate) async fn import_actual_fp_source_v1(descriptor: &FpWireSourceDescriptorV1, requested: &Path) {
    assert_eq!(descriptor.version, 1);
    let rows = true;
    let before_rows = kaspa_p2p_flows::ibd::IbdFlow::last_authenticated_row_snapshot_v1();
    let original = Path::new(&descriptor.source_db).canonicalize().expect("actual closed FP source DB");
    let run = requested.parent().unwrap().canonicalize().unwrap().join(requested.file_name().unwrap());
    assert!(!run.exists() && !run.starts_with(&original));
    let point = descriptor.pruning_point;
    let tip = descriptor.tip;
    let release = descriptor.consumer.birth.block;
    let (mut cfg, bundle, premine, floats) =
        crate::pipeline::virtual_processor::tests::family_supplier_node_candidate::family_native39_campaign_config_v2();
    cfg.palw_row_stream_ibd = true;
    assert!(cfg.params.palw_fork_choice_commitment_v1.is_none(), "ordinary RFC37 current flat-root dialect");
    assert_eq!(cfg.params.genesis.hash, descriptor.genesis);
    assert_eq!(cfg.params.consensus_params_id().to_string(), descriptor.params_id);
    assert_ne!(point, cfg.params.genesis.hash);
    std::fs::create_dir(&run).unwrap();
    let source_root = run.join("source-consensus");
    let target_root = run.join("target-consensus");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&target_root).unwrap();
    let source_management = ConnBuilder::default().with_files_limit(20).with_db_path(run.join("source-management")).build().unwrap();
    let target_management = ConnBuilder::default().with_files_limit(20).with_db_path(run.join("target-management")).build().unwrap();
    let source_path = reserve_existing_entry_v1(source_management.clone(), &source_root);
    let target_path = reserve_existing_entry_v1(target_management.clone(), &target_root);
    // Only scratch source receives the native whole-DB checkpoint. Target is
    // initialized independently with this exact private fixture's genesis alone.
    let source_db =
        ConnBuilder::default().with_files_limit(20).with_db_path(original.clone()).with_create_if_missing(false).build().unwrap();
    rocksdb::checkpoint::Checkpoint::new(&**source_db).unwrap().create_checkpoint(&source_path).unwrap();
    drop(source_db);
    eprintln!("FP_PRUNED_WIRE_V1 source={original:?} original_open_may_recover=true source_must_be_closed=true run={run:?}");
    let seed_db = ConnBuilder::default().with_files_limit(20).with_db_path(target_path).build().unwrap();
    let (seed_tx, _seed_rx) = async_channel::unbounded();
    let seed = t12_genesis_chain_on(TestConsensus::with_db(seed_db.clone(), &cfg, seed_tx), &cfg, &bundle, &premine, &floats);
    assert_eq!(seed.sink(), cfg.params.genesis.hash);
    drop(seed);
    let weak = Arc::downgrade(&seed_db);
    drop(seed_db);
    assert_eq!(weak.strong_count(), 0);
    let (source_tx, _source_rx) = async_channel::unbounded();
    let (target_tx, _target_rx) = async_channel::unbounded();
    let source_notifications = Arc::new(ConsensusNotificationRoot::new(source_tx));
    let target_notifications = Arc::new(ConsensusNotificationRoot::new(target_tx));
    let source_clock = FixtureClockV1::new(descriptor.simulated_time);
    let target_clock = FixtureClockV1::new(descriptor.simulated_time);
    let source_factory = Factory::retained_wire_fixture_v1(
        source_management,
        &cfg,
        source_root,
        source_notifications.clone(),
        &premine,
        cfg.params.genesis.hash,
        &descriptor.params_id,
        source_clock.clone(),
    );
    let target_factory = Factory::retained_wire_fixture_v1(
        target_management,
        &cfg,
        target_root.clone(),
        target_notifications.clone(),
        &premine,
        cfg.params.genesis.hash,
        &descriptor.params_id,
        target_clock.clone(),
    );
    {
        let source = source_factory.retained_fixture_active_directory_v1();
        let target = target_factory.retained_fixture_active_directory_v1();
        assert_eq!(source, target, "both independent management stores begin at their own first entry");
    }
    {
        let actual = source_factory.retained_fixture_active_consensus_v1();
        assert_eq!(actual.get_sink(), descriptor.tip);
        let (_, state) = actual.storage.palw_state_v2_store.read().load_tip_cached(&bundle.state).unwrap().unwrap();
        assert_eq!(state.state_root(), descriptor.tip_root, "source kind binds actual retained tip root");
        observe(&state, &bundle, descriptor);
    }
    let source = WireNode::start(source_factory.clone(), &cfg, source_notifications, run.join("source-addresses"), true);
    let target = WireNode::start(target_factory.clone(), &cfg, target_notifications, run.join("target-addresses"), false);
    let old_directory = target_factory.retained_fixture_active_directory_v1();
    {
        let active = target_factory.retained_fixture_active_consensus_v1();
        assert_eq!(active.get_sink(), cfg.params.genesis.hash);
        assert!(active.get_header(point).is_err(), "target has not received source headers by direct API");
    }
    {
        let active = source_factory.retained_fixture_active_consensus_v1();
        assert_eq!(active.get_sink(), tip);
        assert_eq!(active.pruning_point(), point);
        assert!(!active.storage.block_transactions_store.has(release).unwrap());
        assert!(!active.storage.palw_state_v2_store.read().has_delta(release).unwrap());
    }
    let address = source.address.unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if target.adaptor.connect_peer(address.to_string()).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("real TCP handshake deadline");
    // Read-only metadata from the same production
    // routing constants used by register/request/decode, not fixture integers.
    let (current, complete_snapshot, prefix_history) = kaspa_p2p_flows::flow_context::palw_pruning_protocol_versions_v1();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !source.flow.hub().has_peers() || !target.flow.hub().has_peers() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both actual handshake hub registrations complete");
    for flow in [&source.flow, &target.flow] {
        let peers = flow.hub().active_peers();
        assert!(!peers.is_empty());
        for peer in peers {
            let negotiated = peer.properties().protocol_version;
            let expected = if rows { 109 } else { current };
            assert_eq!(negotiated, expected, "both nodes must negotiate exactly their explicit local opt-in dialect");
            assert!(negotiated >= complete_snapshot, "snapshot must include native complete material and declarations");
            assert!(
                negotiated >= prefix_history,
                "this fixture retains the current history-capable dialect; it does not test history-page retrieval"
            );
            eprintln!(
                "FP_PRUNED_WIRE_V1 negotiated={negotiated} current={current} required_complete={complete_snapshot} history_capability={prefix_history}"
            );
        }
    }
    source.announce(tip).await;
    // This is one finite transfer, not an operational campaign. Keep the
    // existing retained-source bound; an unavailable source is a hard failure.
    let deadline = Instant::now() + Duration::from_secs(7200);
    let mut last = None;
    loop {
        let active = target_factory.retained_fixture_active_consensus_v1();
        let observed = (
            active.get_sink(),
            active.pruning_point(),
            active.is_pruning_utxoset_stable(),
            target_clock.staging_created(),
            target.reset.0.load(Ordering::Relaxed),
        );
        if last != Some(observed) {
            eprintln!("FP_PRUNED_WIRE_V1 target={observed:?} expected_tip={tip} pp={point} ibd={}", target.flow.is_ibd_running());
            last = Some(observed);
        }
        if observed.0 == tip && observed.1 == point && observed.2 && !target.flow.is_ibd_running() {
            break;
        }
        drop(active); // do not retain an old native DB across production staging cleanup
        assert!(Instant::now() < deadline, "actual P2P headers-proof/pruning import did not complete");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(target_clock.staging_created() > 0, "production Factory::new_staging_consensus must actually run");
    assert_ne!(
        target_factory.retained_fixture_active_directory_v1(),
        old_directory,
        "actual durable management publication, not relay-only catchup"
    );
    assert!(target.reset.0.load(Ordering::Relaxed) > 0, "production manager reset handler invoked");
    let target_active = target_factory.retained_fixture_active_consensus_v1();
    let epoch = target_active.pruning_import_session().unwrap();
    assert!(epoch.epoch > 0 && epoch.active.is_none() && !epoch.utxos_started && !epoch.utxos_imported);
    drop(target_active);
    compare_fp_wire_nodes(&source_factory, &target_factory, &bundle, tip, descriptor);
    if rows {
        let receipt =
            kaspa_p2p_flows::ibd::IbdFlow::last_authenticated_row_snapshot_v1().expect("codec2 full inventory was actually received");
        assert!(receipt.0 > before_rows.map_or(0, |old| old.0), "negotiation or codec1 fallback alone cannot pass this fixture");
        assert_eq!(receipt.1, point);
        let source = source_factory.retained_fixture_active_consensus_v1();
        let (snapshot_point, state) = source
            .storage
            .palw_state_v2_store
            .read()
            .load_pruning_snapshot_retained_v1(&bundle.state)
            .unwrap()
            .expect("canonical source snapshot remains servable");
        assert_eq!(snapshot_point, point);
        assert_eq!(state.state_root(), receipt.2, "source matches actual row-authenticated root");
        assert!(
            state.kernel_route_state_v1().is_some(),
            "this is the retained native/material source, not an empty genesis codec fixture"
        );
        assert_eq!(state.state_root(), descriptor.pruning_root, "observed source PP is the actual authenticated generation");
        let actual = observe(&state, &bundle, descriptor);
        assert!(actual.owner9_rows().iter().all(|n| *n > 0));
        assert_eq!(actual.exact_edges(), 1);
        let mut count = 0;
        state
            .visit_direct_state_root_collections_v1(|_, _, _, _| {
                count += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(receipt.3, count, "the real peer delivered the entire canonical inventory");
        eprintln!(
            "FP_RFC37_WIRE_V1 codec=2 generation=1 point={point} state_root={} collections={} actual_receive_sequence={}",
            receipt.2, receipt.3, receipt.0
        );
    }

    let now = descriptor.simulated_time.checked_add(cfg.params.target_time_per_block()).unwrap();
    source_clock.advance(now);
    target_clock.advance(now);
    let active = source_factory.retained_fixture_active_consensus_v1();
    let mut template =
        active.build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard).unwrap();
    template.block.header.nonce = descriptor.nonce.checked_add(1).unwrap();
    template.block.header.finalize();
    let (template, _) = active.heartbeat_adapt_block_template(template).unwrap();
    let block = template.block.to_immutable();
    let next = block.hash();
    let final_clock = now.max(block.header.timestamp);
    source_clock.advance(final_clock);
    target_clock.advance(final_clock);
    assert!(final_clock >= block.header.timestamp, "same explicit validation clock covers the native slot timestamp");
    active.validate_and_insert_block(block).virtual_state_task.await.unwrap();
    drop(active);
    source.announce(next).await;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let active = target_factory.retained_fixture_active_consensus_v1();
        if active.get_sink() == next && active.block_status(next) == BlockStatus::StatusUTXOValid {
            break;
        }
        drop(active);
        assert!(Instant::now() < deadline, "one post-IBD block did not arrive through actual relay");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    compare_fp_wire_nodes(&source_factory, &target_factory, &bundle, next, descriptor);
    let final_directory = target_factory.retained_fixture_active_directory_v1();
    target.stop().await;
    drop(target_factory);
    // Cold check uses real Factory reopening the management-selected directory,
    // rather than a raw DB path chosen before the actual staging publication.
    let management = ConnBuilder::default()
        .with_files_limit(20)
        .with_db_path(run.join("target-management"))
        .with_create_if_missing(false)
        .build()
        .unwrap();
    let (cold_tx, _cold_rx) = async_channel::unbounded();
    let cold_notifications = Arc::new(ConsensusNotificationRoot::new(cold_tx));
    let cold_factory = Factory::retained_wire_fixture_v1(
        management,
        &cfg,
        target_root,
        cold_notifications,
        &premine,
        cfg.params.genesis.hash,
        &descriptor.params_id,
        FixtureClockV1::new(final_clock),
    );
    let cold_manager = ConsensusManager::new(cold_factory.clone()).unwrap();
    assert_eq!(cold_factory.retained_fixture_active_directory_v1(), final_directory);
    compare_fp_wire_nodes(&source_factory, &cold_factory, &bundle, next, descriptor);
    drop(cold_manager);
    drop(cold_factory);
    source.stop().await;
    eprintln!(
        "FP_PRUNED_WIRE_V1 actual_tcp_factory_staging_session_snapshot_cold_and_newblock=PASS rows={rows} run={run:?} full_daemon_args_os_crash_campaign=NOT_RUN"
    );
}

fn observe(
    state: &PalwChainStateV2,
    bundle: &PalwConsensusParamsV2,
    d: &FpWireSourceDescriptorV1,
) -> kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<
    kaspa_consensus_core::palw_state_v2::PalwActualFpCarrierObservationV1,
> {
    kaspa_consensus_core::palw_state_v2::palw_actual_fp_carrier_observation_v1(
        state,
        &bundle.state,
        (d.source.claim, d.source.birth, d.source.outer),
        (d.consumer.claim, d.consumer.birth, d.consumer.outer),
    )
    .unwrap()
}
fn compare_fp_wire_nodes(left: &Factory, right: &Factory, bundle: &PalwConsensusParamsV2, tip: Hash64, d: &FpWireSourceDescriptorV1) {
    let l = left.retained_fixture_active_consensus_v1();
    let r = right.retained_fixture_active_consensus_v1();
    assert_eq!(l.get_sink(), tip);
    assert_eq!(r.get_sink(), tip);
    assert!(r.is_pruning_utxoset_stable());
    let (_, lhs) = l.storage.palw_state_v2_store.read().load_tip_cached(&bundle.state).unwrap().unwrap();
    let (_, rhs) = r.storage.palw_state_v2_store.read().load_tip_cached(&bundle.state).unwrap().unwrap();
    assert_eq!(lhs.state_root(), rhs.state_root());
    assert_eq!(wire_rows(&l), wire_rows(&r));
    let a = observe(&lhs, bundle, d);
    let b = observe(&rhs, bundle, d);
    assert_eq!(*a, *b);
    assert_eq!(a.current_winners(), [Some(d.source.outer), Some(d.consumer.outer)]);
    assert_eq!(b.current_winners(), [Some(d.source.outer), Some(d.consumer.outer)]);
    for native in [&*l, &*r] {
        for e in [&d.source, &d.consumer] {
            assert_eq!(native.palw_fp_accounting_winning_carrier_v1(e.claim).unwrap(), Some(e.outer));
        }
    }
}
