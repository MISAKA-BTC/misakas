//! Child of the existing retained-source native pruned-import fixture.
//! Real TCP flows + production Factory staging/Ctl/Manager. Not full daemon Args startup.
use super::*;
use crate::consensus::factory::{
    Factory,
    retained_source_wire_fixture_v1::{FixtureClockV1, reserve_existing_entry_v1},
};
use crate::pipeline::virtual_processor::tests::{OnetimeTxSelector, new_miner_data};
use kaspa_consensus_core::block::TemplateBuildMode;
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_consensusmanager::{ConsensusManager, ConsensusResetHandler};
use kaspa_core::{core::Core, service::Service, task::tick::TickService};
use kaspa_mining::{
    AttestationMempoolPolicy,
    manager::{MiningManager, MiningManagerProxy},
};
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_p2p_lib::{Adaptor, pb};
use kaspa_p2p_mining::rule_engine::MiningRuleEngine;
use std::{
    net::{Ipv4Addr, SocketAddr, TcpListener},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

#[derive(Default)]
struct ResetCount(AtomicUsize);
impl ConsensusResetHandler for ResetCount {
    fn handle_consensus_reset(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

struct WireNode {
    factory: Arc<Factory>,
    manager: Arc<ConsensusManager>,
    flow: Arc<FlowContext>,
    tick: Arc<TickService>,
    adaptor: Arc<Adaptor>,
    address: Option<SocketAddr>,
    workers: Vec<std::thread::JoinHandle<()>>,
    reset: Arc<ResetCount>,
    _address_db: Arc<kaspa_database::prelude::DB>,
}
impl WireNode {
    fn start(
        factory: Arc<Factory>,
        cfg: &Config,
        notifications: Arc<ConsensusNotificationRoot>,
        address_db: PathBuf,
        listen: bool,
    ) -> Self {
        let manager = Arc::new(ConsensusManager::new(factory.clone()).unwrap());
        let reset = Arc::new(ResetCount::default());
        manager.register_consensus_reset_handler(reset.clone());
        let workers = manager.clone().start(Arc::new(Core::new()));
        let tick = Arc::new(TickService::new());
        let db = ConnBuilder::default().with_files_limit(20).with_db_path(address_db).build().unwrap();
        let config = Arc::new(cfg.clone());
        let (addresses, _) = kaspa_addressmanager::AddressManager::new(config.clone(), db.clone(), tick.clone());
        let pool = MiningManagerProxy::new(Arc::new(MiningManager::new_with_extended_config(
            config.target_time_per_block(),
            false,
            config.max_block_mass,
            config.ram_scale,
            config.block_template_cache_lifetime,
            Default::default(),
            true,
            config.palw_model_market.is_some(),
            config.palw_activation_pool_fence().is_some(),
            None,
            AttestationMempoolPolicy::disabled(),
            kaspa_mining::mempool::config::palw_h1_carrier_priority_for(&config.params),
        )));
        let hub = kaspa_p2p_lib::Hub::new();
        let rules = Arc::new(MiningRuleEngine::new(
            manager.clone(),
            config.clone(),
            Default::default(),
            tick.clone(),
            hub.clone(),
            Default::default(),
            Arc::new(kaspa_core::chain_participation::ChainParticipationGate::new(true)),
        ));
        let flow =
            Arc::new(FlowContext::new(manager.clone(), addresses, config, pool, tick.clone(), notifications, hub.clone(), rules));
        let (adaptor, address) = if listen {
            let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let address = socket.local_addr().unwrap();
            drop(socket);
            (Adaptor::bidirectional(address.into(), hub, flow.clone(), Default::default()).unwrap(), Some(address))
        } else {
            (Adaptor::client_only(hub, flow.clone(), Default::default()), None)
        };
        Self { factory, manager, flow, tick, adaptor, address, workers, reset, _address_db: db }
    }
    async fn announce(&self, block: Hash64) {
        self.flow
            .hub()
            .broadcast(
                pb::KaspadMessage {
                    payload: Some(pb::kaspad_message::Payload::InvRelayBlock(pb::InvRelayBlockMessage { hash: Some(block.into()) })),
                    ..Default::default()
                },
                None,
            )
            .await;
    }
    async fn stop(self) {
        let flow_released = Arc::downgrade(&self.flow);
        let manager_released = Arc::downgrade(&self.manager);
        self.tick.shutdown();
        self.adaptor.close().await;
        drop(self.adaptor);
        self.manager.clone().stop();
        for worker in self.workers {
            worker.join().unwrap();
        }
        drop(self.flow);
        drop(self.manager);
        drop(self.factory);
        drop(self._address_db);
        tokio::time::timeout(Duration::from_secs(10), async {
            while flow_released.strong_count() != 0 || manager_released.strong_count() != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("actual socket/flow/manager owners close before cold reopen");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires completed retained source, explicit new run directory and exclusive source DB closure"]
async fn retained_source_tcp_pruned_ibd_publishes_real_staging_and_relays_one_new_block() {
    let original = required_path("MISAKA_RFC12_PRUNED_SOURCE_DB_V1").canonicalize().expect("completed source required");
    let requested = required_path("MISAKA_RFC12_PRUNED_WIRE_RUN_V1");
    let run = requested.parent().unwrap().canonicalize().unwrap().join(requested.file_name().unwrap());
    assert!(!run.exists() && !run.starts_with(&original));
    let inputs = original.join("source-fixture-input-v1");
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(inputs.join("manifest.json")).expect("complete source manifest")).unwrap();
    validate_inputs(&inputs, &manifest);
    let reader: ReaderInput = borsh::from_slice(&std::fs::read(inputs.join("reader-input")).unwrap()).unwrap();
    let admission: SignedAdmission = borsh::from_slice(&std::fs::read(inputs.join("signed-admission")).unwrap()).unwrap();
    let replacement: SignedReplacement = borsh::from_slice(&std::fs::read(inputs.join("signed-replacement")).unwrap()).unwrap();
    assert_eq!(admission.request, reader.request.id());
    let root = reader.request.intent.manifest.root;
    assert_eq!(root.to_string(), manifest.material_root);
    let consents = [admission.consent, replacement.consent];
    let point: Hash64 = manifest.pruning_point.parse().unwrap();
    let tip: Hash64 = manifest.tip.parse().unwrap();
    let release: Hash64 = manifest.release.parse().unwrap();
    let (cfg, bundle, premine, floats) = config();
    assert_eq!(cfg.params.genesis.hash.to_string(), manifest.genesis);
    assert_eq!(cfg.params.consensus_params_id().to_string(), manifest.consensus_params_id);
    assert_ne!(point, cfg.params.genesis.hash);
    assert_eq!(reader.expected_tip, tip);
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
    eprintln!("PRUNED_WIRE_V1 source={original:?} original_open_may_recover=true source_must_be_closed=true run={run:?}");
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
    let source_clock = FixtureClockV1::new(reader.simulated_time);
    let target_clock = FixtureClockV1::new(reader.simulated_time);
    let source_factory = Factory::retained_wire_fixture_v1(
        source_management,
        &cfg,
        source_root,
        source_notifications.clone(),
        &premine,
        cfg.params.genesis.hash,
        &manifest.consensus_params_id,
        source_clock.clone(),
    );
    let target_factory = Factory::retained_wire_fixture_v1(
        target_management,
        &cfg,
        target_root.clone(),
        target_notifications.clone(),
        &premine,
        cfg.params.genesis.hash,
        &manifest.consensus_params_id,
        target_clock.clone(),
    );
    {
        let source = source_factory.retained_fixture_active_directory_v1();
        let target = target_factory.retained_fixture_active_directory_v1();
        assert_eq!(source, target, "both independent management stores begin at their own first entry");
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
            assert_eq!(negotiated, current, "same-build native fixture cannot silently fall back to an older dialect");
            assert!(negotiated >= complete_snapshot, "snapshot must include native complete material and declarations");
            assert!(
                negotiated >= prefix_history,
                "this fixture retains the current history-capable dialect; it does not test history-page retrieval"
            );
            eprintln!(
                "PRUNED_WIRE_V1 negotiated={negotiated} current={current} required_complete={complete_snapshot} history_capability={prefix_history}"
            );
        }
    }
    source.announce(tip).await;
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
            eprintln!("PRUNED_WIRE_V1 target={observed:?} expected_tip={tip} pp={point} ibd={}", target.flow.is_ibd_running());
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
    compare_wire_nodes(&source_factory, &target_factory, &bundle, tip, root, &consents);

    let now = reader.simulated_time.checked_add(cfg.params.target_time_per_block()).unwrap();
    source_clock.advance(now);
    target_clock.advance(now);
    let active = source_factory.retained_fixture_active_consensus_v1();
    let mut template =
        active.build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard).unwrap();
    template.block.header.nonce = reader.nonce.checked_add(1).unwrap();
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
    compare_wire_nodes(&source_factory, &target_factory, &bundle, next, root, &consents);
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
        &manifest.consensus_params_id,
        FixtureClockV1::new(final_clock),
    );
    let cold_manager = ConsensusManager::new(cold_factory.clone()).unwrap();
    assert_eq!(cold_factory.retained_fixture_active_directory_v1(), final_directory);
    compare_wire_nodes(&source_factory, &cold_factory, &bundle, next, root, &consents);
    drop(cold_manager);
    drop(cold_factory);
    source.stop().await;
    eprintln!(
        "PRUNED_WIRE_V1 actual_tcp_factory_staging_session_snapshot_cold_and_newblock=PASS run={run:?} full_daemon_args_os_crash_campaign=NOT_RUN"
    );
}

fn wire_rows(native: &crate::consensus::Consensus) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let mut rows = vec![];
    let mut from = None;
    loop {
        let page = native.get_virtual_utxos(from, 4096, from.is_some());
        if page.is_empty() {
            break;
        }
        from = page.last().map(|(out, _)| *out);
        rows.extend(page);
    }
    rows.sort_unstable_by_key(|(out, _)| (out.transaction_id, out.index));
    rows
}
fn compare_wire_nodes(
    left: &Factory,
    right: &Factory,
    bundle: &PalwConsensusParamsV2,
    tip: Hash64,
    root: Hash64,
    consents: &[Hash64; 2],
) {
    let left = left.retained_fixture_active_consensus_v1();
    let right = right.retained_fixture_active_consensus_v1();
    assert_eq!(left.get_sink(), tip);
    assert_eq!(right.get_sink(), tip);
    assert!(right.is_pruning_utxoset_stable());
    let (_, lhs) = left.storage.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    let (_, rhs) = right.storage.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    assert_eq!(lhs.state_root(), rhs.state_root());
    assert_eq!(wire_rows(&left), wire_rows(&right));
    for consent in consents {
        let lhs = left.palw_material_terminal_receipt_finalized_v2(root, *consent).unwrap().expect("source finalized receipt");
        let rhs = right.palw_material_terminal_receipt_finalized_v2(root, *consent).unwrap().expect("wire-imported finalized receipt");
        assert!(rhs.1.covers(root, *consent));
        assert_eq!(lhs, rhs);
    }
}
