//! Actual gRPC requests served by RpcCoreService over the independently P2P-validated node.
//! A pinned validated header authenticates the returned state facts and carrier transactions.
use super::*;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_state_proof_v1::{state_root_of_pinned_header_v1, verify_claim_v1};
use kaspa_core::task::service::AsyncService;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwLegacyDisputeRequest, GetPalwStateProofRequest};

pub(super) async fn verify_socket_evidence(
    chain: &Chain,
    flow: Arc<FlowContext>,
    rules: Arc<MiningRuleEngine>,
    claim: Hash64,
    floor: u64,
) {
    let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(chain.tc.clone()))));
    let context = SubscriptionContext::default();
    // This read-only test does not request notification subscriptions. The normal notifier/core
    // service lifecycle still runs; all RPC data comes from the independently validated database.
    let (_notification_sender, notification_receiver) = async_channel::unbounded();
    let notify = Arc::new(kaspa_consensus_notify::service::NotifyService::new(
        chain.tc.notification_root(),
        notification_receiver,
        context.clone(),
    ));
    let tick = Arc::new(TickService::new());
    let perf = Arc::new(kaspa_perf_monitor::builder::Builder::new().with_tick_service(tick).build());
    let config = Arc::new(chain.config.clone());
    let core = Arc::new(kaspa_rpc_service::service::RpcCoreService::new(
        manager,
        notify.notifier(),
        None,
        flow.mining_manager().clone(),
        flow,
        context,
        None,
        config.clone(),
        Arc::new(kaspa_core::core::Core::new()),
        chain.tc.processing_counters().clone(),
        Default::default(),
        Default::default(),
        perf,
        Default::default(),
        Default::default(),
        kaspa_utils::sysinfo::SystemInfo::default(),
        rules,
        None,
    ));
    let notified = notify.clone();
    let notify_task = tokio::spawn(async move { notified.start().await.unwrap() });
    let running = core.clone();
    let core_task = tokio::spawn(async move { running.start().await.unwrap() });
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let server = kaspa_grpc_server::adaptor::Adaptor::server(
        address.into(),
        config.bps(),
        kaspa_grpc_server::manager::Manager::new(4),
        core.clone(),
        core.notifier(),
        core.subscription_context(),
        1,
        Default::default(),
    );
    let client = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match GrpcClient::connect(format!("grpc://{address}")).await {
                Ok(client) => break client,
                Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .expect("a real bounded gRPC connection completes");
    let pinned = chain.tc.get_sink();
    let response = client
        .get_palw_state_proof(GetPalwStateProofRequest { block_hash: pinned.to_string(), collection: "claims".to_string() })
        .await
        .expect("the ordinary gRPC route transports the claim collection proof");
    assert!(response.available, "RPC cannot establish the pinned state: {}", response.reason);
    assert_eq!(response.block_hash, pinned.to_string());
    let header = Header::try_from(response.header.as_ref().unwrap()).unwrap();
    let root = state_root_of_pinned_header_v1(&header, pinned).expect("re-hashed RPC header is the independently validated pin");
    let proof = misaka_palw_remote::proof::proof_from_parts_v1(
        response.state_preimage,
        &response.collection,
        response.rows.into_iter().map(|row| (row.key, row.value)).collect(),
    );
    let authenticated = verify_claim_v1(&proof, root, &claim).expect("public RPC facts match the pinned committed collection");
    assert_eq!(authenticated.execution_root, chain.view(claim).execution_root);
    assert_eq!(authenticated.trace_root, chain.view(claim).trace_root);
    let mut forged = proof.clone();
    forged.collection.rows[0].1[0] ^= 1;
    assert!(verify_claim_v1(&forged, root, &claim).is_err(), "changed RPC rows must not authenticate");
    let status = client.get_palw_legacy_dispute(GetPalwLegacyDisputeRequest { claim_id: claim.to_string() }).await.unwrap();
    assert_eq!(status, kaspa_rpc_core::convert::palw_legacy::palw_legacy_dispute_response_v1(chain.tc.as_ref().as_ref(), claim));
    let mut manifests = 0;
    let mut parts = BTreeSet::new();
    let mut folds = BTreeSet::new();
    let mut cursor = Some(pinned);
    let mut pages = 0;
    while let Some(start) = cursor {
        pages += 1;
        assert!(pages <= 64, "bounded native selected-chain history must progress");
        cursor = crate::palw_panel::walk_accepted_lifecycle_page_with_block_v2(
            chain.tc.as_ref().as_ref(),
            start,
            floor,
            64,
            &mut |fold, object| {
                let wanted = match object {
                    PalwConsensusObjectV2::LegacyHeldAnswerDeclaredV2 { declaration } => {
                        declaration.claim == claim && declaration.unit == PalwLegacyHeldUnitV2::FpJob
                    }
                    PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk } => {
                        chunk.claim == claim && chunk.unit == PalwLegacyHeldUnitV2::FpJob
                    }
                    _ => false,
                };
                if wanted {
                    folds.insert(fold);
                }
            },
        )
        .expect("native acceptance locates independently validated public carriers");
        assert_ne!(cursor, Some(start));
    }
    let mut blocks = BTreeSet::new();
    for fold in folds {
        for merged in chain.tc.get_block_acceptance_data(fold).unwrap().iter() {
            if !merged.accepted_transactions.is_empty() {
                blocks.insert(merged.block_hash);
            }
        }
    }
    for block in blocks {
        let remote = client.get_block(block, true).await.expect("real RPC supplies public multipart carrier bytes");
        let header = Header::try_from(&remote.header).unwrap();
        assert_eq!(kaspa_consensus_core::hashing::header::hash(&header), block);
        let txs: Vec<Transaction> = remote.transactions.into_iter().map(|tx| tx.try_into().unwrap()).collect();
        assert_eq!(kaspa_consensus_core::merkle::calc_hash_merkle_root(txs.iter()), header.hash_merkle_root);
        for tx in txs {
            let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) else { continue };
            match payload.object {
                PalwConsensusObjectV2::LegacyHeldAnswerDeclaredV2 { declaration }
                    if declaration.claim == claim && declaration.unit == PalwLegacyHeldUnitV2::FpJob =>
                {
                    manifests += 1;
                }
                PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk }
                    if chunk.claim == claim && chunk.unit == PalwLegacyHeldUnitV2::FpJob =>
                {
                    assert!(parts.insert(chunk.index), "authenticated RPC history contains no repeated accepted part");
                }
                _ => {}
            }
        }
    }
    assert_eq!(manifests, 1);
    assert_eq!(parts.len(), 2);
    eprintln!("[service-network] actual socket gRPC authenticated the pinned claim and signed multipart carriers");
    client.disconnect().await.unwrap();
    server.stop().await.unwrap();
    core.clone().signal_exit();
    tokio::time::timeout(Duration::from_secs(10), core_task).await.unwrap().unwrap();
    notify.clone().signal_exit();
    tokio::time::timeout(Duration::from_secs(10), notify_task).await.unwrap().unwrap();
}
