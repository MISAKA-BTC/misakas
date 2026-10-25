//! Private fault-provider process plus the unmodified production checker loop.
use super::*;
use kaspa_consensus::consensus::test_consensus::{TestConsensus, TestConsensusFactory};
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::tick::TickService;
use kaspa_mining::manager::{MiningManager, MiningManagerProxy};
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_p2p_mining::rule_engine::MiningRuleEngine;
use std::io::{Read, Write};
#[path = "../../../../consensus/tests/fixtures/signed_provider_config_v4.rs"]
mod common;
fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}
fn dir() -> Option<PathBuf> {
    std::env::var_os("MISAKA_SIGNED_PROVIDER_HANDOFF_V4")
        .map(|path| std::fs::canonicalize(path).expect("existing canonical fixture directory"))
}
fn child_name(name: &str) -> String {
    format!("{}::{name}", module_path!().split_once("::").unwrap().1)
}
struct ProviderChild(std::process::Child);
impl Drop for ProviderChild {
    fn drop(&mut self) {
        // A failing checker must not leave a loopback fixture or an inherited
        // output pipe alive after its parent has already reported failure.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn provider_child() {
    if std::env::var("MISAKA_SIGNED_PROVIDER_SERVER_V4").as_deref() != Ok("1") {
        return;
    }
    use kaspa_consensus_core::palw_kernel_material_query_v4::{
        PalwKernelMaterialUnitQueryV4 as Q, PalwKernelMaterialUnitResponseV4 as R,
    };
    use misaka_palw_remote::provider_transport_v1::*;
    let dir = dir().unwrap();
    let input: common::CheckerHandoff = borsh::from_slice(&std::fs::read(dir.join("checker-handoff")).unwrap()).unwrap();
    assert_ne!(std::process::id(), input.writer_pid);
    let domain: Hash64 = borsh::from_slice(&std::fs::read(dir.join("checker-domain")).unwrap()).unwrap();
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed(common::seed(input.provider_card as usize));
    let checker = kaspa_pq_validator_core::ValidatorKey::from_seed(common::seed(input.checker_card as usize));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    std::fs::write(dir.join("provider-pid"), std::process::id().to_string()).unwrap();
    std::fs::write(dir.join("provider-address.tmp"), listener.local_addr().unwrap().to_string()).unwrap();
    std::fs::rename(dir.join("provider-address.tmp"), dir.join("provider-address")).unwrap();
    let policy = ProviderTransportPolicyV1::new(domain, PALW_KERNEL_MATERIAL_QUERY_FRAME_BYTES_V4);
    let mut replay = ProviderTransportReplayGuardV1::new(32, 120_000);
    // The malformed authenticated bytes are intentional Byzantine fixture behavior.
    // Production provider-store validation is not bypassed or made configurable.
    for index in 0..2 {
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        let (mut socket, _) = loop {
            match listener.accept() {
                Ok(pair) => break pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline, "fixture request did not arrive");
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("fixture accept: {error}"),
            }
        };
        socket.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
        socket.set_write_timeout(Some(Duration::from_secs(15))).unwrap();
        let mut len = [0u8; 8];
        socket.read_exact(&mut len).unwrap();
        let len = u64::from_le_bytes(len);
        assert!(len <= 65536);
        let mut bytes = vec![0; len as usize];
        socket.read_exact(&mut bytes).unwrap();
        let request: ProviderTransportRequestV1 = borsh::from_slice(&bytes).unwrap();
        provider_transport_verify_request_v1(
            &request,
            ProviderTransportPurposeV1::KernelMaterialQueryV4,
            &policy,
            now(),
            Some(input.daa),
            &mut replay,
        )
        .unwrap();
        assert_eq!(request.header.sender, input.checker);
        assert_eq!(request.header.sender_pubkey, checker.public_key());
        assert_eq!(request.header.recipients, vec![input.provider]);
        let query: Q = borsh::from_slice(&request.body).unwrap();
        query.validate().unwrap();
        assert_eq!(
            (query.root, query.dependency, query.consent, query.provider),
            (input.root, input.root, input.consent, input.provider)
        );
        let body = borsh::to_vec(&R::Answer { query: query.clone(), bytes: vec![0] }).unwrap();
        let signer = |m: &[u8], c: &[u8]| Ok(key.sign_with_context(m, c).to_vec());
        let response = provider_transport_response_v1(&request.header, now(), body, vec![(input.provider, &signer)]).unwrap();
        let bytes = borsh::to_vec(&response).unwrap();
        socket.write_all(&(bytes.len() as u64).to_le_bytes()).unwrap();
        socket.write_all(&bytes).unwrap();
        socket.flush().unwrap();
        std::fs::write(dir.join(format!("provider-query-{index}")), borsh::to_vec(&query).unwrap()).unwrap();
    }
}
#[tokio::test]
async fn checker_child() {
    let Some(dir) = dir() else {
        return;
    };
    let input: common::CheckerHandoff = borsh::from_slice(&std::fs::read(dir.join("checker-handoff")).unwrap()).unwrap();
    assert_ne!(std::process::id(), input.writer_pid);
    let (mut config, _, _, _) = common::config();
    config.process_genesis = false;
    let config = Arc::new(config);
    let db = kaspa_database::prelude::ConnBuilder::default()
        .with_files_limit(10)
        .with_db_path(PathBuf::from(std::env::var_os("MISAKA_SIGNED_PROVIDER_DB_V4").unwrap()))
        .with_create_if_missing(false)
        .build()
        .unwrap();
    let (notify, keep_rx) = async_channel::unbounded();
    let node = Arc::new(TestConsensus::with_db(db.clone(), &config, notify));
    let workers = node.init();
    assert_eq!(node.get_sink(), input.checkpoint);
    assert_eq!(node.get_virtual_daa_score(), input.daa);
    let route = node.palw_kernel_route_v1().unwrap();
    let domain = Hash64::from_bytes(route.header.policy.network_domain);
    assert_eq!(material_provider_generation_from_route_v4(&route, input.root, input.provider).unwrap(), Some(input.consent));
    let source = sources(route, input.enrollment.as_bytes(), 0).unwrap();
    assert!(source.lock().seats.contains(&input.checker));
    std::fs::write(dir.join("checker-domain"), borsh::to_vec(&domain).unwrap()).unwrap();
    let mut server = ProviderChild(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &child_name("provider_child"), "--nocapture", "--test-threads=1"])
            .env("MISAKA_SIGNED_PROVIDER_SERVER_V4", "1")
            .spawn()
            .unwrap(),
    );
    let stop = std::time::Instant::now() + Duration::from_secs(15);
    while !dir.join("provider-address").exists() {
        assert!(std::time::Instant::now() < stop);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let pid: u32 = std::fs::read_to_string(dir.join("provider-pid")).unwrap().parse().unwrap();
    assert_ne!(pid, std::process::id());
    assert_ne!(pid, input.writer_pid);
    let address = std::fs::read_to_string(dir.join("provider-address")).unwrap();
    let checker_key = kaspa_pq_validator_core::ValidatorKey::from_seed(common::seed(input.checker_card as usize));
    let client = AuthenticatedSocketExchangeV3::kernel_reader_v4(
        address.parse().unwrap(),
        misaka_palw_remote::provider_transport_v1::ProviderTransportPolicyV1::new(domain, PALW_KERNEL_MATERIAL_QUERY_FRAME_BYTES_V4),
        input.checker,
        kaspa_pq_validator_core::ValidatorKey::from_seed(common::seed(input.checker_card as usize)),
    );
    let work: ProgramPaidBootstrapWorkV3 = borsh::from_slice(&std::fs::read(dir.join("checker-work")).unwrap()).unwrap();
    let transport_failed = std::cell::Cell::new(false);
    let error = source
        .verify_using_provider_located_v4(
            &work.material,
            ProviderReaderLimitsV4 {
                max_unit_bytes: 1 << 20,
                max_total_fetch_bytes: 4 << 20,
                max_workspace_bytes: 128 << 20,
                max_units: 65536,
            },
            |stage, position| {
                let query = PalwKernelMaterialUnitQueryV4 {
                    version: 4,
                    root: input.root,
                    consent: input.consent,
                    dependency: input.root,
                    provider: input.provider,
                    stage,
                    position,
                };
                let authority = node
                    .palw_kernel_material_query_authority_v4(&query, input.checker, checker_key.public_key())
                    .map_err(ProviderReaderErrorV4::Unknown)?;
                client.fetch_kernel_material_unit_classified_v4(&query, &authority).map_err(|e| {
                    transport_failed.set(true);
                    e
                })
            },
        )
        .err()
        .expect("genuine signed socket unit must fail actual SDK validation");
    assert!(!transport_failed.get(), "signature/echo/authenticated transport must succeed before SDK failure");
    let unit = provider_material_challenge_unit_v4(&error).expect("exact unit decoding/authentication provenance");
    assert!(matches!(
        error.into_cause(),
        kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialFetchErrorV4::InvalidMaterial(_)
    ));
    // Full daemon checker context: no copied binder/sign/funding or authority reconstruction.
    let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(node.clone()))).unwrap());
    let tick = Arc::new(TickService::new());
    let (_meta_life, meta) = kaspa_database::create_temp_db!(kaspa_database::prelude::ConnBuilder::default().with_files_limit(10));
    let (addresses, _) = kaspa_addressmanager::AddressManager::new(config.clone(), meta, tick.clone());
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
        kaspa_mining::AttestationMempoolPolicy::disabled(),
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
    let flow = Arc::new(FlowContext::new(
        manager.clone(),
        addresses,
        config.clone(),
        pool,
        tick.clone(),
        node.notification_root(),
        hub,
        rules,
    ));
    let key_path = dir.join("checker-key");
    let mut key_options = std::fs::OpenOptions::new();
    key_options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        key_options.mode(0o600);
    }
    key_options.open(&key_path).unwrap()
        .write_all(faster_hex::hex_string(&common::seed(input.checker_card as usize)).as_bytes()).unwrap();
    let spec_path = dir.join("checker-spec.json");
    let outpoint = |p: TransactionOutpoint| format!("{}:{}", p.transaction_id, p.index);
    std::fs::write(&spec_path,serde_json::to_vec(&serde_json::json!({"version":4,"work_path":dir.join("checker-work"),"enrollment":input.enrollment.to_string(),"round":0,"checker_bond":outpoint(input.checker.0),"checker_key":key_path,"provider_bond":outpoint(input.provider.0),"provider_address":address,"payer_key":key_path,"funding_outpoints":[outpoint(input.funding)],"maximum_fee_sompi":100_000_000u64,"max_input_bytes":16<<20,"max_unit_bytes":1<<20,"max_total_fetch_bytes":4<<20,"max_workspace_bytes":128<<20,"max_requests":65536,"challenge_valid_until_daa":input.daa+64})).unwrap()).unwrap();
    let ctx = PalwNativeCustodyContextV3 {
        consensus_manager: manager,
        flow_context: flow,
        tick_service: tick.clone(),
        recovery_provider_roster_v4: vec![],
        config: config.clone(),
        data_dir: dir.join("node-outbox"),
        class_artifacts: vec![],
        class_cache_bytes: 1 << 20,
        chain_classes: false,
        producer_key_path: None,
        producer_bond: None,
    };
    let prepared = PrivateKernelPaidCheckSpecV4::read(&spec_path).unwrap().prepare(&ctx).unwrap();
    let (_, object_path, tx_path) = outbox::action_paths(&prepared.outbox, 0);
    // Production runs this checker on its own runtime thread. Its retained
    // relation consumer is deliberately Rc, so poll it on this same thread.
    let running = prepared.run();
    tokio::pin!(running);
    let stop = std::time::Instant::now() + Duration::from_secs(30);
    let observe_wal = async {
        while !tx_path.exists() {
            assert!(std::time::Instant::now() < stop, "production checker produced no funded carrier");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::select! {
        result = &mut running => panic!("production checker exited before transaction WAL: {result:?}"),
        () = observe_wal => {},
    }
    tick.shutdown();
    running.await.unwrap();
    assert!(server.0.wait().unwrap().success());
    let object: PalwConsensusObjectV2 = borsh::from_slice(&std::fs::read(&object_path).unwrap()).unwrap();
    let PalwConsensusObjectV2::MonetaryMaterialActionV1 { action, .. } = &object else { panic!("actual monetary challenge") };
    let kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::ChallengeCurrentV4 { stage, position, .. } =
        &**action
    else {
        panic!("availability obligation")
    };
    assert_eq!((*stage, *position), unit);
    std::fs::copy(object_path, dir.join("checker-object")).unwrap();
    std::fs::copy(tx_path, dir.join("checker-transaction")).unwrap();
    eprintln!("SIGNED_UNIT_NATIVE_V4 checker_pid={} provider_pid={pid} sdk_unit={unit:?} production_wal=ready", std::process::id());
    drop(client);
    drop(source);
    drop(ctx);
    drop(keep_rx);
    node.shutdown(workers);
    drop(node);
    drop(db);
}
