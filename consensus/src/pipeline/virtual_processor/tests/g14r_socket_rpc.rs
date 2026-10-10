//! **Lane G14-R: the node's PALW read ops over a REAL socket** — a gRPC server and a wRPC proxy (borsh or JSON) on 127.0.0.1, in
//! process, in front of a test node.
//!
//! [`PalwSocketNode`] is an `RpcApi` whose PALW ops are exactly what the RPC service's handlers run (`rpc/service/src/service.rs`):
//! the request parsers and response builders of `kaspa_rpc_core::convert::{palw_kernel, palw_legacy}` and the consensus API's
//! `palw_state_proof_v1`, against the node it fronts — ops 202 (state proof), 204–206 (legacy dispute, disputes, filer status), 210–212
//! (kernel claim, rows, finals) and 231 (conformance evidence) — plus `getSink`, `getInfo` and `submitTransaction` (the node's mempool
//! check, then its template; the harness mines). The full `RpcCoreService` needs a p2p flow context, a mining manager and an index
//! processor the test node does not run, and its network must pass `validate_palw_v2`, which refuses every fence these worlds arm;
//! everything ELSE on the path is the shipping code: `kaspa_grpc_server::Adaptor` (tonic, the protowire conversions), the wRPC proxy
//! (`kaspa_wrpc_server` in `grpc_proxy_address` mode: the `workflow_rpc` websocket server, borsh or serde-JSON framing) and the real
//! clients (`kaspa_wrpc_client::KaspaRpcClient`, `kaspa_grpc_client::GrpcClient`).
//!
//! The node behind the socket can be swapped ([`PalwSocketNode::serve`]) — a restart, an IBD node, an imported node — and the wRPC
//! proxy can be stopped and restarted on the same port ([`SocketRpc::restart_proxy`]): a connected client is dropped and reconnects.
#![allow(dead_code)]
use async_channel::{Receiver, unbounded};
use async_trait::async_trait;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::tx::Transaction;
use kaspa_grpc_server::{adaptor::Adaptor, manager::Manager};
use kaspa_notify::events::EVENT_TYPE_ARRAY;
use kaspa_notify::listener::{ListenerId, ListenerLifespan};
use kaspa_notify::notifier::Notifier;
use kaspa_notify::scope::Scope;
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_notify::subscription::{MutationPolicies, UtxosChangedMutationPolicy};
use kaspa_rpc_core::{RpcResult, notify::connection::ChannelConnection};
use kaspa_rpc_core::{api::connection::DynRpcConnection, api::rpc::RpcApi, *};
use kaspa_utils::networking::{ContextualNetAddress, NetAddress};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use parking_lot::Mutex;
use std::sync::Arc;

type RpcCoreNotifier = Notifier<Notification, ChannelConnection>;

/// **The test node behind the socket**, swappable; and the transactions `submitTransaction` admitted, for the harness's template.
pub(super) struct PalwSocketNode {
    /// Weak: the test owns the node (its `TestConsensus` asserts at shutdown that nothing else holds it).
    node: Mutex<std::sync::Weak<crate::consensus::Consensus>>,
    pending: Mutex<Vec<Transaction>>,
    core_notifier: Arc<RpcCoreNotifier>,
    _sync_receiver: Receiver<()>,
}

impl PalwSocketNode {
    pub(super) fn new(node: Arc<crate::consensus::Consensus>) -> Arc<Self> {
        let (sync_sender, sync_receiver) = unbounded();
        let policies = MutationPolicies::new(UtxosChangedMutationPolicy::AddressSet);
        let core_notifier: Arc<RpcCoreNotifier> = Arc::new(Notifier::with_sync(
            "palw-socket-node",
            EVENT_TYPE_ARRAY[..].into(),
            vec![],
            vec![],
            SubscriptionContext::new(),
            10,
            policies,
            Some(sync_sender),
        ));
        core_notifier.clone().start();
        Arc::new(Self {
            node: Mutex::new(Arc::downgrade(&node)),
            pending: Mutex::new(Vec::new()),
            core_notifier,
            _sync_receiver: sync_receiver,
        })
    }

    /// The socket now fronts `node` (a restarted, IBD or imported node): what was pending in the old node's mempool is gone with it.
    pub(super) fn serve(&self, node: Arc<crate::consensus::Consensus>) {
        *self.node.lock() = Arc::downgrade(&node);
        self.pending.lock().clear();
    }

    fn node(&self) -> Arc<crate::consensus::Consensus> {
        self.node.lock().upgrade().expect("the node behind the socket is running")
    }

    /// What `submitTransaction` admitted since the last call (the node's template takes them).
    pub(super) fn take_pending(&self) -> Vec<Transaction> {
        std::mem::take(&mut *self.pending.lock())
    }

    async fn blocking<T: Send + 'static>(&self, f: impl FnOnce(&crate::consensus::Consensus) -> T + Send + 'static) -> T {
        let node = self.node();
        tokio::task::spawn_blocking(move || f(node.as_ref())).await.expect("the read joins")
    }
}

/// **A gRPC server and a wRPC proxy in front of a [`PalwSocketNode`]**, on 127.0.0.1.
pub(super) struct SocketRpc {
    pub(super) node: Arc<PalwSocketNode>,
    grpc: Arc<Adaptor>,
    proxy: Option<workflow_rpc::server::RpcServer>,
    pub(super) wrpc_port: u16,
    encoding: WrpcEncoding,
}

fn free_port() -> u16 {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap().port()
}

impl SocketRpc {
    /// Start both servers in front of `node`, the wRPC proxy speaking `encoding` (borsh or JSON).
    pub(super) async fn start(node: Arc<crate::consensus::Consensus>, encoding: WrpcEncoding) -> SocketRpc {
        let node = PalwSocketNode::new(node);
        let grpc_address: NetAddress = ContextualNetAddress::unspecified().normalize(free_port());
        let service: Arc<dyn RpcApi> = node.clone();
        let grpc = Adaptor::server(
            grpc_address,
            1,
            Manager::new(128),
            service,
            node.core_notifier.clone(),
            node.core_notifier.subscription_context().clone(),
            3,
            Default::default(),
        );
        let mut rpc = SocketRpc { node, grpc, proxy: None, wrpc_port: free_port(), encoding };
        rpc.start_proxy().await;
        rpc
    }

    async fn start_proxy(&mut self) {
        use kaspa_wrpc_server::{
            connection::Connection,
            router::Router,
            server::Server,
            service::{KaspaRpcHandler, Options},
        };
        use workflow_rpc::server::prelude::*;
        let options = Arc::new(Options {
            listen_address: format!("wrpc://127.0.0.1:{}", self.wrpc_port),
            grpc_proxy_address: Some(format!("grpc://127.0.0.1:{}", self.grpc.serve_address().port)),
            verbose: false,
        });
        let handler = Arc::new(KaspaRpcHandler::new(2, self.encoding, None, options.clone()));
        let router = Arc::new(Router::new(handler.server.clone()));
        let server = RpcServer::new_with_encoding::<Server, Connection, kaspa_rpc_core::api::ops::RpcApiOps, Id64>(
            self.encoding,
            handler.clone(),
            router.interface.clone(),
            Some(Arc::new(WebSocketCounters::default())),
            false,
        );
        // The port may linger a moment after a stop: retry the bind.
        let mut listener = None;
        for _ in 0..100 {
            match server.bind(&options.listen_address).await {
                Ok(l) => {
                    listener = Some(l);
                    break;
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }
        let listener = listener.expect("the wRPC proxy binds its port");
        let serving = server.clone();
        tokio::spawn(async move {
            let config = WebSocketConfig { max_message_size: Some(1 << 30), ..Default::default() };
            let _ = serving.listen(listener, Some(config)).await;
        });
        self.proxy = Some(server);
    }

    /// **The proxy goes away and comes back on the same port** (a node's RPC restart): connected clients are dropped.
    pub(super) async fn restart_proxy(&mut self) {
        if let Some(proxy) = self.proxy.take() {
            proxy.stop_and_join().await.expect("the proxy stops");
        }
        self.start_proxy().await;
    }

    /// A real wRPC client of the proxy (borsh or JSON, as the proxy speaks).
    pub(super) async fn client(&self) -> KaspaRpcClient {
        let url = format!("ws://127.0.0.1:{}", self.wrpc_port);
        let client = KaspaRpcClient::new(self.encoding, Some(&url), None, None, None).expect("a client");
        let options = ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(std::time::Duration::from_secs(10)),
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        };
        let mut last = None;
        for _ in 0..40 {
            match client.connect(Some(options.clone())).await {
                Ok(_) => return client,
                Err(e) => {
                    last = Some(e.to_string());
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
        panic!("the wRPC client never connected: {last:?}");
    }

    /// A real gRPC client of the server.
    pub(super) async fn grpc_client(&self) -> kaspa_grpc_client::GrpcClient {
        let url = format!("grpc://localhost:{}", self.grpc.serve_address().port);
        let mut last = None;
        for _ in 0..40 {
            match kaspa_grpc_client::GrpcClient::connect(url.clone()).await {
                Ok(c) => return c,
                Err(e) => {
                    last = Some(e.to_string());
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
        panic!("the gRPC client never connected: {last:?}");
    }

    pub(super) async fn stop(mut self) {
        if let Some(proxy) = self.proxy.take() {
            let _ = proxy.stop_and_join().await;
        }
        let _ = self.grpc.stop().await;
    }
}

#[async_trait]
impl RpcApi for PalwSocketNode {
    async fn get_info_call(&self, _connection: Option<&DynRpcConnection>, _request: GetInfoRequest) -> RpcResult<GetInfoResponse> {
        Ok(GetInfoResponse {
            p2p_id: "palw-socket-node".to_string(),
            mempool_size: self.pending.lock().len() as u64,
            server_version: "g14r".to_string(),
            is_utxo_indexed: false,
            is_synced: true,
            has_notify_command: true,
            has_message_id: true,
        })
    }

    async fn get_sink_call(&self, _connection: Option<&DynRpcConnection>, _request: GetSinkRequest) -> RpcResult<GetSinkResponse> {
        Ok(GetSinkResponse { sink: self.blocking(|c| c.get_sink()).await })
    }

    /// The node's mempool check (`validate_mempool_transaction`), then the harness's template.
    async fn submit_transaction_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitTransactionRequest,
    ) -> RpcResult<SubmitTransactionResponse> {
        let tx = Transaction::try_from(request.transaction)?;
        let checked = tx.clone();
        let verdict = self
            .blocking(move |c| {
                c.validate_mempool_transaction(
                    &mut kaspa_consensus_core::tx::MutableTransaction::from_tx(checked),
                    &Default::default(),
                )
            })
            .await;
        verdict.map_err(|e| RpcError::General(format!("the mempool refuses the transaction: {e}")))?;
        let id = tx.id();
        let mut pending = self.pending.lock();
        if !pending.iter().any(|t| t.id() == id) {
            pending.push(tx);
        }
        Ok(SubmitTransactionResponse { transaction_id: id })
    }

    // ---- op 202: a PALW state collection proven against the header that commits it ----
    async fn get_palw_state_proof_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwStateProofRequest,
    ) -> RpcResult<GetPalwStateProofResponse> {
        let collection = request.collection.trim().to_ascii_lowercase();
        if !matches!(collection.as_str(), "bonds" | "classes" | "claims") {
            return Err(RpcError::General(format!("collection '{}' is not one of bonds, classes, claims", request.collection)));
        }
        let named = match request.block_hash.trim() {
            "" => None,
            text => Some(text.parse::<kaspa_hashes::Hash64>().map_err(|e| RpcError::General(format!("blockHash: {e}")))?),
        };
        let wanted = collection.clone().into_bytes();
        let served = self
            .blocking(move |c| {
                let block = named.unwrap_or_else(|| c.get_sink());
                c.palw_state_proof_v1(block, &wanted).map(|p| (block, p))
            })
            .await;
        Ok(match served {
            Err(reason) => GetPalwStateProofResponse { available: false, reason, ..Default::default() },
            Ok((block, (header, proof))) => GetPalwStateProofResponse {
                available: true,
                reason: String::new(),
                block_hash: block.to_string(),
                header: Some(RpcHeader::from(&header)),
                state_preimage: proof.state.preimage,
                collection,
                rows: proof.collection.rows.into_iter().map(|(key, value)| RpcPalwProofRow { key, value }).collect(),
            },
        })
    }

    // ---- ops 204–206: the legacy route's public dispute reads ----
    async fn get_palw_legacy_dispute_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwLegacyDisputeRequest,
    ) -> RpcResult<GetPalwLegacyDisputeResponse> {
        use kaspa_rpc_core::convert::palw_legacy::{palw_legacy_dispute_request_v1, palw_legacy_dispute_response_v1};
        let claim = palw_legacy_dispute_request_v1(&request)?;
        Ok(self.blocking(move |c| palw_legacy_dispute_response_v1(c, claim)).await)
    }

    async fn get_palw_legacy_disputes_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwLegacyDisputesRequest,
    ) -> RpcResult<GetPalwLegacyDisputesResponse> {
        use kaspa_rpc_core::convert::palw_legacy::{palw_legacy_disputes_request_v1, palw_legacy_disputes_response_v1};
        let (reserver, limit) = palw_legacy_disputes_request_v1(&request)?;
        Ok(self.blocking(move |c| palw_legacy_disputes_response_v1(c, reserver, limit)).await)
    }

    async fn get_palw_fraud_filer_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwFraudFilerStatusRequest,
    ) -> RpcResult<GetPalwFraudFilerStatusResponse> {
        use kaspa_rpc_core::convert::palw_legacy::{palw_fraud_filer_status_request_v1, palw_fraud_filer_status_response_v1};
        let bond = palw_fraud_filer_status_request_v1(&request)?;
        Ok(self.blocking(move |c| palw_fraud_filer_status_response_v1(c, bond)).await)
    }

    // ---- ops 210–212 and 231: the kernel route ----
    async fn get_palw_kernel_claim_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwKernelClaimRequest,
    ) -> RpcResult<GetPalwKernelClaimResponse> {
        let claim = kaspa_rpc_core::convert::palw_kernel::palw_kernel_claim_request_v1(&request)?;
        self.blocking(move |c| kaspa_rpc_core::convert::palw_kernel::palw_kernel_claim_response_v1(c, claim)).await
    }

    async fn get_palw_kernel_rows_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwKernelRowsRequest,
    ) -> RpcResult<GetPalwKernelRowsResponse> {
        let (after, max_bytes) = kaspa_rpc_core::convert::palw_kernel::palw_kernel_rows_request_v1(&request)?;
        self.blocking(move |c| kaspa_rpc_core::convert::palw_kernel::palw_kernel_rows_response_v1(c, after, max_bytes)).await
    }

    async fn get_palw_kernel_finals_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwKernelFinalsRequest,
    ) -> RpcResult<GetPalwKernelFinalsResponse> {
        let limit = kaspa_rpc_core::convert::palw_kernel::palw_kernel_finals_request_v1(&request);
        self.blocking(move |c| kaspa_rpc_core::convert::palw_kernel::palw_kernel_finals_response_v1(c, limit)).await
    }

    async fn get_palw_conformance_evidence_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwConformanceEvidenceRequest,
    ) -> RpcResult<GetPalwConformanceEvidenceResponse> {
        let class = kaspa_rpc_core::convert::palw_kernel::palw_conformance_evidence_request_v1(&request)?;
        self.blocking(move |c| kaspa_rpc_core::convert::palw_kernel::palw_conformance_evidence_response_v1(c, class)).await
    }

    // ---- everything else: `RpcCoreMock`'s answers (`rpc/grpc/server/src/tests/rpc_core_mock.rs`) ----
    async fn ping_call(&self, _connection: Option<&DynRpcConnection>, _request: PingRequest) -> RpcResult<PingResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_metrics_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetMetricsRequest,
    ) -> RpcResult<GetMetricsResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_connections_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetConnectionsRequest,
    ) -> RpcResult<GetConnectionsResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_system_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSystemInfoRequest,
    ) -> RpcResult<GetSystemInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_server_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetServerInfoRequest,
    ) -> RpcResult<GetServerInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_sync_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSyncStatusRequest,
    ) -> RpcResult<GetSyncStatusResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_current_network_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetCurrentNetworkRequest,
    ) -> RpcResult<GetCurrentNetworkResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn submit_block_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: SubmitBlockRequest,
    ) -> RpcResult<SubmitBlockResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_template_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBlockTemplateRequest,
    ) -> RpcResult<GetBlockTemplateResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_peer_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetPeerAddressesRequest,
    ) -> RpcResult<GetPeerAddressesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_mempool_entry_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetMempoolEntryRequest,
    ) -> RpcResult<GetMempoolEntryResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_mempool_entries_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetMempoolEntriesRequest,
    ) -> RpcResult<GetMempoolEntriesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_connected_peer_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetConnectedPeerInfoRequest,
    ) -> RpcResult<GetConnectedPeerInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn submit_transaction_replacement_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: SubmitTransactionReplacementRequest,
    ) -> RpcResult<SubmitTransactionReplacementResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn add_peer_call(&self, _connection: Option<&DynRpcConnection>, _request: AddPeerRequest) -> RpcResult<AddPeerResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_call(&self, _connection: Option<&DynRpcConnection>, _request: GetBlockRequest) -> RpcResult<GetBlockResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_subnetwork_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSubnetworkRequest,
    ) -> RpcResult<GetSubnetworkResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_virtual_chain_from_block_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetVirtualChainFromBlockRequest,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_blocks_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBlocksRequest,
    ) -> RpcResult<GetBlocksResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_current_block_color_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetCurrentBlockColorRequest,
    ) -> RpcResult<GetCurrentBlockColorResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_count_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBlockCountRequest,
    ) -> RpcResult<GetBlockCountResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_dag_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBlockDagInfoRequest,
    ) -> RpcResult<GetBlockDagInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn resolve_finality_conflict_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn shutdown_call(&self, _connection: Option<&DynRpcConnection>, _request: ShutdownRequest) -> RpcResult<ShutdownResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_headers_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetHeadersRequest,
    ) -> RpcResult<GetHeadersResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_balance_by_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBalanceByAddressRequest,
    ) -> RpcResult<GetBalanceByAddressResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_balances_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_utxos_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetUtxosByAddressesRequest,
    ) -> RpcResult<GetUtxosByAddressesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_utxos_by_address_page_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetUtxosByAddressPageRequest,
    ) -> RpcResult<GetUtxosByAddressPageResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_sink_blue_score_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSinkBlueScoreRequest,
    ) -> RpcResult<GetSinkBlueScoreResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn ban_call(&self, _connection: Option<&DynRpcConnection>, _request: BanRequest) -> RpcResult<BanResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn unban_call(&self, _connection: Option<&DynRpcConnection>, _request: UnbanRequest) -> RpcResult<UnbanResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn estimate_network_hashes_per_second_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_mempool_entries_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_coin_supply_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetCoinSupplyRequest,
    ) -> RpcResult<GetCoinSupplyResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_daa_score_timestamp_estimate_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetDaaScoreTimestampEstimateRequest,
    ) -> RpcResult<GetDaaScoreTimestampEstimateResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_fee_estimate_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetFeeEstimateRequest,
    ) -> RpcResult<GetFeeEstimateResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_fee_estimate_experimental_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetFeeEstimateExperimentalRequest,
    ) -> RpcResult<GetFeeEstimateExperimentalResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_utxo_return_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetUtxoReturnAddressRequest,
    ) -> RpcResult<GetUtxoReturnAddressResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_virtual_chain_from_block_v2_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetVirtualChainFromBlockV2Request,
    ) -> RpcResult<GetVirtualChainFromBlockV2Response> {
        Err(RpcError::NotImplemented)
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId {
        self.core_notifier.register_new_listener(connection, ListenerLifespan::Dynamic)
    }

    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        self.core_notifier.unregister_listener(id)?;
        Ok(())
    }

    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.core_notifier.try_start_notify(id, scope)?;
        Ok(())
    }

    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.core_notifier.try_stop_notify(id, scope)?;
        Ok(())
    }
}
