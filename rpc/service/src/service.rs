//! Core server implementation for ClientAPI

use super::collector::{CollectorFromConsensus, CollectorFromIndex};
use crate::converter::feerate_estimate::{FeeEstimateConverter, FeeEstimateVerboseConverter};
use crate::converter::{consensus::ConsensusConverter, index::IndexConverter, protocol::ProtocolConverter};
use async_trait::async_trait;
use kaspa_consensus_core::api::counters::ProcessingCounters;
use kaspa_consensus_core::daa_score_timestamp::DaaScoreTimestamp;
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::tx::{TransactionQueryResult, TransactionType};
use kaspa_consensus_core::utxo::utxo_inquirer::UtxoInquirerError;
use kaspa_consensus_core::{
    block::Block,
    coinbase::MinerData,
    config::Config,
    constants::MAX_SOMPI,
    network::NetworkType,
    tx::{COINBASE_TRANSACTION_INDEX, Transaction},
};
use kaspa_consensus_notify::{
    notifier::ConsensusNotifier,
    {connection::ConsensusChannelConnection, notification::Notification as ConsensusNotification},
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::time::unix_now;
use kaspa_core::{
    core::Core,
    debug,
    kaspad_env::version,
    signals::Shutdown,
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    task::tick::TickService,
    trace, warn,
};
use kaspa_index_core::indexed_utxos::BalanceByScriptPublicKey;
use kaspa_index_core::{
    connection::IndexChannelConnection, indexed_utxos::UtxoSetByScriptPublicKey, notification::Notification as IndexNotification,
    notifier::IndexNotifier,
};
use kaspa_mining::feerate::FeeEstimateVerbose;
use kaspa_mining::model::tx_query::TransactionQuery;
use kaspa_mining::{manager::MiningManagerProxy, mempool::tx::Orphan};
use kaspa_notify::listener::ListenerLifespan;
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_notify::subscription::{MutationPolicies, UtxosChangedMutationPolicy};
use kaspa_notify::{
    collector::DynCollector,
    connection::ChannelType,
    events::{EVENT_TYPE_ARRAY, EventSwitches, EventType},
    listener::ListenerId,
    notifier::Notifier,
    scope::Scope,
    subscriber::{Subscriber, SubscriptionManager},
};
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_p2p_lib::common::ProtocolError;
use kaspa_p2p_mining::rule_engine::MiningRuleEngine;
use kaspa_perf_monitor::{Monitor as PerfMonitor, counters::CountersSnapshot};
use kaspa_rpc_core::{
    Notification, RpcError, RpcResult,
    api::{
        connection::DynRpcConnection,
        ops::{RPC_API_REVISION, RPC_API_VERSION},
        rpc::{MAX_SAFE_WINDOW_SIZE, RpcApi},
    },
    model::*,
    notify::connection::ChannelConnection,
};
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script};
use kaspa_utils::expiring_cache::ExpiringCache;
use kaspa_utils::sysinfo::SystemInfo;
use kaspa_utils::{channel::Channel, triggers::SingleTrigger};
use kaspa_utils_tower::counters::TowerConnectionCounters;
use kaspa_utxoindex::api::UtxoIndexProxy;
use std::time::{Duration, Instant};
use std::{
    collections::HashMap,
    iter::once,
    sync::{Arc, atomic::Ordering},
    vec,
};
use tokio::join;
use workflow_rpc::server::WebSocketCounters as WrpcServerCounters;

/// A service implementing the Rpc API at kaspa_rpc_core level.
///
/// Collects notifications from the consensus and forwards them to
/// actual protocol-featured services. Thanks to the subscription pattern,
/// notifications are sent to the registered services only if the actually
/// need them.
///
/// ### Implementation notes
///
/// This was designed to have a unique instance in the whole application,
/// though multiple instances could coexist safely.
///
/// Any lower-level service providing an actual protocol, like gPRC should
/// register into this instance in order to get notifications. The data flow
/// from this instance to registered services and backwards should occur
/// by adding respectively to the registered service a Collector and a
/// Subscriber.
/// kaspa-pq Phase 11 (ADR-0010): bridges the in-process validator service (defined in
/// the `kaspad` crate) to the RPC layer without a circular dependency — `kaspad`
/// implements this trait for its `ValidatorService`, and `RpcCoreService` holds an
/// optional `dyn` to serve `getValidatorStatus`.
#[async_trait]
pub trait ValidatorStatusProvider: Send + Sync {
    async fn rpc_validator_status(&self) -> GetValidatorStatusResponse;
}

/// Parse a "txid_hex:index" stake-bond outpoint (txid = 64-byte Hash64) for the
/// kaspa-pq Phase 12 (ADR-0011) validator RPCs. A malformed value is a client error.
fn parse_bond_outpoint(s: &str) -> RpcResult<kaspa_consensus_core::tx::TransactionOutpoint> {
    let (txid, index) = s.split_once(':').ok_or_else(|| RpcError::General(format!("bond outpoint '{s}' must be 'txid_hex:index'")))?;
    let transaction_id: kaspa_hashes::Hash64 =
        txid.parse().map_err(|_| RpcError::General(format!("bond outpoint '{s}' has an invalid 64-byte txid")))?;
    let index: u32 = index.parse().map_err(|_| RpcError::General(format!("bond outpoint '{s}' has a non-numeric index")))?;
    Ok(kaspa_consensus_core::tx::TransactionOutpoint::new(transaction_id, index))
}

/// kaspa-pq EVM Lane v0.4 (§16): parse a 32-byte EVM tx hash (hex, optional 0x).
fn parse_evm_tx_hash(s: &str) -> RpcResult<kaspa_hashes::EvmH256> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if h.len() != 64 {
        return Err(RpcError::RpcSubsystem(format!("evm tx hash must be 64 hex chars, got {}", h.len())));
    }
    let mut bytes = [0u8; 32];
    faster_hex::hex_decode(h.as_bytes(), &mut bytes).map_err(|e| RpcError::RpcSubsystem(format!("malformed evm tx hash: {e}")))?;
    Ok(kaspa_hashes::EvmH256::from_bytes(bytes))
}

pub struct RpcCoreService {
    consensus_manager: Arc<ConsensusManager>,
    notifier: Arc<Notifier<Notification, ChannelConnection>>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    utxoindex: Option<UtxoIndexProxy>,
    config: Arc<Config>,
    consensus_converter: Arc<ConsensusConverter>,
    index_converter: Arc<IndexConverter>,
    protocol_converter: Arc<ProtocolConverter>,
    core: Arc<Core>,
    processing_counters: Arc<ProcessingCounters>,
    wrpc_borsh_counters: Arc<WrpcServerCounters>,
    wrpc_json_counters: Arc<WrpcServerCounters>,
    shutdown: SingleTrigger,
    core_shutdown_request: SingleTrigger,
    perf_monitor: Arc<PerfMonitor<Arc<TickService>>>,
    p2p_tower_counters: Arc<TowerConnectionCounters>,
    grpc_tower_counters: Arc<TowerConnectionCounters>,
    system_info: SystemInfo,
    fee_estimate_cache: ExpiringCache<RpcFeeEstimate>,
    fee_estimate_verbose_cache: ExpiringCache<kaspa_mining::errors::MiningManagerResult<GetFeeEstimateExperimentalResponse>>,
    mining_rule_engine: Arc<MiningRuleEngine>,
    /// kaspa-pq Phase 11: optional bridge to the in-process validator service.
    validator_status_provider: Option<Arc<dyn ValidatorStatusProvider>>,
}

const RPC_CORE: &str = "rpc-core";

impl RpcCoreService {
    pub const IDENT: &'static str = "rpc-core-service";

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        consensus_notifier: Arc<ConsensusNotifier>,
        index_notifier: Option<Arc<IndexNotifier>>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
        subscription_context: SubscriptionContext,
        utxoindex: Option<UtxoIndexProxy>,
        config: Arc<Config>,
        core: Arc<Core>,
        processing_counters: Arc<ProcessingCounters>,
        wrpc_borsh_counters: Arc<WrpcServerCounters>,
        wrpc_json_counters: Arc<WrpcServerCounters>,
        perf_monitor: Arc<PerfMonitor<Arc<TickService>>>,
        p2p_tower_counters: Arc<TowerConnectionCounters>,
        grpc_tower_counters: Arc<TowerConnectionCounters>,
        system_info: SystemInfo,
        mining_rule_engine: Arc<MiningRuleEngine>,
        validator_status_provider: Option<Arc<dyn ValidatorStatusProvider>>,
    ) -> Self {
        // This notifier UTXOs subscription granularity to index-processor or consensus notifier
        let policies = match index_notifier {
            Some(_) => MutationPolicies::new(UtxosChangedMutationPolicy::AddressSet),
            None => MutationPolicies::new(UtxosChangedMutationPolicy::Wildcard),
        };

        // Prepare consensus-notify objects
        let consensus_notify_channel = Channel::<ConsensusNotification>::default();
        let consensus_notify_listener_id = consensus_notifier.register_new_listener(
            ConsensusChannelConnection::new(RPC_CORE, consensus_notify_channel.sender(), ChannelType::Closable),
            ListenerLifespan::Static(Default::default()),
        );

        // Prepare the rpc-core notifier objects
        let mut consensus_events: EventSwitches = EVENT_TYPE_ARRAY[..].into();
        consensus_events[EventType::UtxosChanged] = false;
        consensus_events[EventType::PruningPointUtxoSetOverride] = index_notifier.is_none();
        let consensus_converter = Arc::new(ConsensusConverter::new(consensus_manager.clone(), config.clone()));
        let consensus_collector = Arc::new(CollectorFromConsensus::new(
            "rpc-core <= consensus",
            consensus_notify_channel.receiver(),
            consensus_converter.clone(),
        ));
        let consensus_subscriber =
            Arc::new(Subscriber::new("rpc-core => consensus", consensus_events, consensus_notifier, consensus_notify_listener_id));

        let mut collectors: Vec<DynCollector<Notification>> = vec![consensus_collector];
        let mut subscribers = vec![consensus_subscriber];

        // Prepare index-processor objects if an IndexService is provided
        let index_converter = Arc::new(IndexConverter::new(config.clone()));
        if let Some(ref index_notifier) = index_notifier {
            let index_notify_channel = Channel::<IndexNotification>::default();
            let index_notify_listener_id = index_notifier.clone().register_new_listener(
                IndexChannelConnection::new(RPC_CORE, index_notify_channel.sender(), ChannelType::Closable),
                ListenerLifespan::Static(policies),
            );

            let index_events: EventSwitches = [EventType::UtxosChanged, EventType::PruningPointUtxoSetOverride].as_ref().into();
            let index_collector =
                Arc::new(CollectorFromIndex::new("rpc-core <= index", index_notify_channel.receiver(), index_converter.clone()));
            let index_subscriber =
                Arc::new(Subscriber::new("rpc-core => index", index_events, index_notifier.clone(), index_notify_listener_id));

            collectors.push(index_collector);
            subscribers.push(index_subscriber);
        }

        // Protocol converter
        let protocol_converter = Arc::new(ProtocolConverter::new(flow_context.clone()));

        // Create the rcp-core notifier
        let notifier =
            Arc::new(Notifier::new(RPC_CORE, EVENT_TYPE_ARRAY[..].into(), collectors, subscribers, subscription_context, 1, policies));

        Self {
            consensus_manager,
            notifier,
            mining_manager,
            flow_context,
            utxoindex,
            config,
            consensus_converter,
            index_converter,
            protocol_converter,
            core,
            processing_counters,
            wrpc_borsh_counters,
            wrpc_json_counters,
            shutdown: SingleTrigger::default(),
            core_shutdown_request: SingleTrigger::default(),
            perf_monitor,
            p2p_tower_counters,
            grpc_tower_counters,
            system_info,
            fee_estimate_cache: ExpiringCache::new(Duration::from_millis(500), Duration::from_millis(1000)),
            fee_estimate_verbose_cache: ExpiringCache::new(Duration::from_millis(500), Duration::from_millis(1000)),
            mining_rule_engine,
            validator_status_provider,
        }
    }

    pub fn start_impl(&self) {
        self.notifier().start();
    }

    pub async fn join(&self) -> RpcResult<()> {
        trace!("{} joining notifier", Self::IDENT);
        self.notifier().join().await?;
        Ok(())
    }

    #[inline(always)]
    pub fn notifier(&self) -> Arc<Notifier<Notification, ChannelConnection>> {
        self.notifier.clone()
    }

    #[inline(always)]
    pub fn subscription_context(&self) -> SubscriptionContext {
        self.notifier.subscription_context().clone()
    }

    pub fn core_shutdown_request_listener(&self) -> triggered::Listener {
        self.core_shutdown_request.listener.clone()
    }

    async fn get_utxo_set_by_script_public_key<'a>(
        &self,
        addresses: impl Iterator<Item = &'a RpcAddress>,
    ) -> UtxoSetByScriptPublicKey {
        self.utxoindex
            .clone()
            .unwrap()
            .get_utxos_by_script_public_keys(addresses.map(pay_to_address_script).collect())
            .await
            .unwrap_or_default()
    }

    async fn get_balance_by_script_public_key<'a>(&self, addresses: impl Iterator<Item = &'a RpcAddress>) -> BalanceByScriptPublicKey {
        self.utxoindex
            .clone()
            .unwrap()
            .get_balance_by_script_public_keys(addresses.map(pay_to_address_script).collect())
            .await
            .unwrap_or_default()
    }

    fn extract_tx_query(&self, filter_transaction_pool: bool, include_orphan_pool: bool) -> RpcResult<TransactionQuery> {
        match (filter_transaction_pool, include_orphan_pool) {
            (true, true) => Ok(TransactionQuery::OrphansOnly),
            // Note that the first `true` indicates *filtering* transactions and the second `false` indicates not including
            // orphan txs -- hence the query would be empty by definition and is thus useless
            (true, false) => Err(RpcError::InconsistentMempoolTxQuery),
            (false, true) => Ok(TransactionQuery::All),
            (false, false) => Ok(TransactionQuery::TransactionsOnly),
        }
    }
}

#[async_trait]
impl RpcApi for RpcCoreService {
    async fn submit_block_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitBlockRequest,
    ) -> RpcResult<SubmitBlockResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let sink_daa_score_timestamp = session.async_get_sink_daa_score_timestamp().await;

        // do not attempt to submit blocks while in unstable ibd state.
        if session.async_is_consensus_in_transitional_ibd_state().await {
            return Err(RpcError::ConsensusInTransitionalIbdState);
        }

        // TODO: consider adding an error field to SubmitBlockReport to document both the report and error fields
        let is_synced = self.mining_rule_engine.should_mine(sink_daa_score_timestamp);

        if !self.config.enable_unsynced_mining && !is_synced {
            // error = "Block not submitted - node is not synced"
            return Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::IsInIBD) });
        }

        let try_block: RpcResult<Block> = request.block.try_into();
        if let Err(err) = &try_block {
            trace!("incoming SubmitBlockRequest with block conversion error: {}", err);
            // error = format!("Could not parse block: {0}", err)
            return Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) });
        }
        let block = try_block?;
        let hash = block.hash();

        if !request.allow_non_daa_blocks {
            let virtual_daa_score = session.get_virtual_daa_score();

            // A simple heuristic check which signals that the mined block is out of date
            // and should not be accepted unless user explicitly requests.
            let difficulty_window_duration = self.config.difficulty_window_duration_in_block_units();
            if virtual_daa_score > difficulty_window_duration
                && block.header.daa_score < virtual_daa_score - difficulty_window_duration
            {
                // error = format!("Block rejected. Reason: block DAA score {0} is too far behind virtual's DAA score {1}", block.header.daa_score, virtual_daa_score)
                return Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) });
            }
        }

        trace!("incoming SubmitBlockRequest for block {}", hash);
        match self.flow_context.submit_rpc_block(&session, block.clone()).await {
            Ok(_) => Ok(SubmitBlockResponse { report: SubmitBlockReport::Success }),
            Err(ProtocolError::RuleError(RuleError::BadMerkleRoot(h1, h2))) => {
                warn!(
                    "The RPC submitted block {} triggered a {} error: {}.
NOTE: This error usually indicates an RPC conversion error between the node and the miner. This is likely to reflect using a NON-SUPPORTED miner.",
                    hash,
                    stringify!(RuleError::BadMerkleRoot),
                    RuleError::BadMerkleRoot(h1, h2)
                );
                if self.config.net.is_mainnet() {
                    warn!("Printing the full block for debug purposes:\n{:?}", block);
                }
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) })
            }
            Err(err) => {
                warn!("The RPC submitted block triggered an error: {}\nPrinting the full block for debug purposes:\n{:?}", err, block);
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) })
            }
        }
    }

    async fn get_block_template_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetBlockTemplateRequest,
    ) -> RpcResult<GetBlockTemplateResponse> {
        trace!("incoming GetBlockTemplate request");

        if *self.config.net == NetworkType::Mainnet && !self.config.enable_mainnet_mining {
            return Err(RpcError::General("Mining on mainnet is not supported for initial Rust versions".to_owned()));
        }

        // Make sure the pay address prefix matches the config network type
        if request.pay_address.prefix != self.config.prefix() {
            return Err(kaspa_addresses::AddressError::InvalidPrefix(request.pay_address.prefix.to_string()))?;
        }

        // kaspa-pq PQ-only: the miner pay address must be ML-DSA-87 P2PKH. A legacy / ECDSA / P2SH
        // pay address would place a non-PQ miner script in the coinbase payload, which the PQ-only
        // consensus rule rejects (incl. the coinbase-payload check) — the mined block would be dead
        // on arrival and its reward would poison descendants' fan-out. Reject the request up front so
        // the miner gets a clear error instead of an unminable template.
        if request.pay_address.version != kaspa_addresses::Version::PubKeyHashMlDsa87 {
            return Err(RpcError::InvalidRpcScriptClass(
                "pay address must be an ML-DSA-87 P2PKH (PubKeyHashMlDsa87) address".to_owned(),
            ));
        }

        // Build block template
        let session = self.consensus_manager.consensus().unguarded_session();

        // do not attempt to mine blocks while in unstable ibd state.
        if session.async_is_consensus_in_transitional_ibd_state().await {
            return Err(RpcError::ConsensusInTransitionalIbdState);
        }
        let script_public_key = kaspa_txscript::pay_to_address_script(&request.pay_address);
        let extra_data = version().as_bytes().iter().chain(once(&(b'/'))).chain(&request.extra_data).cloned().collect::<Vec<_>>();
        let miner_data: MinerData = MinerData::new(script_public_key, extra_data);
        let block_template = self.mining_manager.clone().get_block_template(&session, miner_data).await?;

        // Check coinbase tx payload length
        if block_template.block.transactions[COINBASE_TRANSACTION_INDEX].payload.len() > self.config.max_coinbase_payload_len {
            return Err(RpcError::CoinbasePayloadLengthAboveMax(self.config.max_coinbase_payload_len));
        }

        Ok(GetBlockTemplateResponse {
            block: block_template.block.into(),
            is_synced: self.mining_rule_engine.should_mine(DaaScoreTimestamp {
                timestamp: block_template.selected_parent_timestamp,
                daa_score: block_template.selected_parent_daa_score,
            }),
        })
    }

    async fn get_current_block_color_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetCurrentBlockColorRequest,
    ) -> RpcResult<GetCurrentBlockColorResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();

        match session.async_get_current_block_color(request.hash).await {
            Some(blue) => Ok(GetCurrentBlockColorResponse { blue }),
            None => Err(RpcError::MergerNotFound(request.hash)),
        }
    }

    async fn get_block_call(&self, _connection: Option<&DynRpcConnection>, request: GetBlockRequest) -> RpcResult<GetBlockResponse> {
        // TODO: test
        let session = self.consensus_manager.consensus().session().await;
        let block = session.async_get_block_even_if_header_only(request.hash).await?;
        Ok(GetBlockResponse {
            block: self
                .consensus_converter
                .get_block(&session, &block, request.include_transactions, request.include_transactions)
                .await?,
        })
    }

    async fn get_blocks_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetBlocksRequest,
    ) -> RpcResult<GetBlocksResponse> {
        // Validate that user didn't set include_transactions without setting include_blocks
        if !request.include_blocks && request.include_transactions {
            return Err(RpcError::InvalidGetBlocksRequest);
        }

        let session = self.consensus_manager.consensus().session().await;

        // If low_hash is empty - use genesis instead.
        let low_hash = match request.low_hash {
            Some(low_hash) => {
                // Make sure low_hash points to an existing and valid block
                session.async_get_ghostdag_data(low_hash).await?;
                low_hash
            }
            None => self.config.genesis.hash,
        };

        // Get hashes between low_hash and sink
        let sink_hash = session.async_get_sink().await;

        // We use +1 because low_hash is also returned
        // max_blocks MUST be >= mergeset_size_limit + 1
        let max_blocks = self.config.mergeset_size_limit() as usize + 1;
        let (block_hashes, high_hash) = session.async_get_hashes_between(low_hash, sink_hash, max_blocks).await?;

        // If the high hash is equal to sink it means get_hashes_between didn't skip any hashes, and
        // there's space to add the sink anticone, otherwise we cannot add the anticone because
        // there's no guarantee that all of the anticone root ancestors will be present.
        let filtered_sink_anticone = if high_hash == sink_hash {
            // Get the sink anticone and filter out duplicates: remove low_hash and any blocks already in block_hashes
            // This prevents the bug where low_hash appears twice (once at the start and once in sink_anticone)
            let sink_anticone = session.async_get_anticone(sink_hash).await?;
            let mut seen_hashes: std::collections::HashSet<_> = once(low_hash).chain(block_hashes.iter().copied()).collect();
            sink_anticone.into_iter().filter(|hash| seen_hashes.insert(*hash)).collect()
        } else {
            vec![]
        };

        // Prepend low hash to make it inclusive and append the filtered sink anticone
        let block_hashes = once(low_hash).chain(block_hashes).chain(filtered_sink_anticone).collect::<Vec<_>>();
        let blocks = if request.include_blocks {
            let mut blocks = Vec::with_capacity(block_hashes.len());
            for hash in block_hashes.iter().copied() {
                let block = session.async_get_block_even_if_header_only(hash).await?;
                let rpc_block = self
                    .consensus_converter
                    .get_block(&session, &block, request.include_transactions, request.include_transactions)
                    .await?;
                blocks.push(rpc_block)
            }
            blocks
        } else {
            Vec::new()
        };
        Ok(GetBlocksResponse { block_hashes, blocks })
    }

    async fn get_info_call(&self, _connection: Option<&DynRpcConnection>, _request: GetInfoRequest) -> RpcResult<GetInfoResponse> {
        let sink_daa_score_timestamp =
            self.consensus_manager.consensus().unguarded_session().async_get_sink_daa_score_timestamp().await;
        Ok(GetInfoResponse {
            p2p_id: self.flow_context.node_id.to_string(),
            mempool_size: self.mining_manager.transaction_count_sample(TransactionQuery::TransactionsOnly),
            server_version: version().to_string(),
            is_utxo_indexed: self.config.utxoindex,
            is_synced: self.mining_rule_engine.is_sink_recent_and_connected(sink_daa_score_timestamp),
            has_notify_command: true,
            has_message_id: true,
        })
    }

    async fn get_mempool_entry_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetMempoolEntryRequest,
    ) -> RpcResult<GetMempoolEntryResponse> {
        let query = self.extract_tx_query(request.filter_transaction_pool, request.include_orphan_pool)?;
        let Some(transaction) = self.mining_manager.clone().get_transaction(request.transaction_id, query).await else {
            return Err(RpcError::TransactionNotFound(request.transaction_id));
        };
        let session = self.consensus_manager.consensus().unguarded_session();
        Ok(GetMempoolEntryResponse::new(self.consensus_converter.get_mempool_entry(&session, &transaction)))
    }

    async fn get_mempool_entries_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetMempoolEntriesRequest,
    ) -> RpcResult<GetMempoolEntriesResponse> {
        let query = self.extract_tx_query(request.filter_transaction_pool, request.include_orphan_pool)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let (transactions, orphans) = self.mining_manager.clone().get_all_transactions(query).await;
        let mempool_entries = transactions
            .iter()
            .chain(orphans.iter())
            .map(|transaction| self.consensus_converter.get_mempool_entry(&session, transaction))
            .collect();
        Ok(GetMempoolEntriesResponse::new(mempool_entries))
    }

    async fn get_mempool_entries_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse> {
        let query = self.extract_tx_query(request.filter_transaction_pool, request.include_orphan_pool)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let script_public_keys = request.addresses.iter().map(pay_to_address_script).collect();
        let grouped_txs = self.mining_manager.clone().get_transactions_by_addresses(script_public_keys, query).await;
        let mempool_entries = grouped_txs
            .owners
            .iter()
            .map(|(script_public_key, owner_transactions)| {
                let address = extract_script_pub_key_address(script_public_key, self.config.prefix())
                    .expect("script public key is convertible into an address");
                self.consensus_converter.get_mempool_entries_by_address(
                    &session,
                    address,
                    owner_transactions,
                    &grouped_txs.transactions,
                )
            })
            .collect();
        Ok(GetMempoolEntriesByAddressesResponse::new(mempool_entries))
    }

    async fn submit_transaction_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitTransactionRequest,
    ) -> RpcResult<SubmitTransactionResponse> {
        let allow_orphan = self.config.unsafe_rpc && request.allow_orphan;
        if !self.config.unsafe_rpc && request.allow_orphan {
            debug!(
                "SubmitTransaction RPC command called with AllowOrphan enabled while node in safe RPC mode -- switching to ForbidOrphan."
            );
        }

        let transaction: Transaction = request.transaction.try_into()?;
        let transaction_id = transaction.id();
        let session = self.consensus_manager.consensus().unguarded_session();
        let orphan = match allow_orphan {
            true => Orphan::Allowed,
            false => Orphan::Forbidden,
        };
        self.flow_context.submit_rpc_transaction(&session, transaction, orphan).await.map_err(|err| {
            let err = RpcError::RejectedTransaction(transaction_id, err.to_string());
            debug!("{err}");
            err
        })?;
        Ok(SubmitTransactionResponse::new(transaction_id))
    }

    async fn submit_transaction_replacement_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitTransactionReplacementRequest,
    ) -> RpcResult<SubmitTransactionReplacementResponse> {
        let transaction: Transaction = request.transaction.try_into()?;
        let transaction_id = transaction.id();
        let session = self.consensus_manager.consensus().unguarded_session();
        let replaced_transaction =
            self.flow_context.submit_rpc_transaction_replacement(&session, transaction).await.map_err(|err| {
                let err = RpcError::RejectedTransaction(transaction_id, err.to_string());
                debug!("{err}");
                err
            })?;
        Ok(SubmitTransactionReplacementResponse::new(transaction_id, (&*replaced_transaction).into()))
    }

    async fn get_current_network_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetCurrentNetworkRequest,
    ) -> RpcResult<GetCurrentNetworkResponse> {
        Ok(GetCurrentNetworkResponse::new(*self.config.net))
    }

    async fn get_subnetwork_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetSubnetworkRequest,
    ) -> RpcResult<GetSubnetworkResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_sink_call(&self, _connection: Option<&DynRpcConnection>, _: GetSinkRequest) -> RpcResult<GetSinkResponse> {
        Ok(GetSinkResponse::new(self.consensus_manager.consensus().unguarded_session().async_get_sink().await))
    }

    async fn get_sink_blue_score_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetSinkBlueScoreRequest,
    ) -> RpcResult<GetSinkBlueScoreResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        Ok(GetSinkBlueScoreResponse::new(session.async_get_ghostdag_data(session.async_get_sink().await).await?.blue_score))
    }

    async fn get_dns_confirmation_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetDnsConfirmationRequest,
    ) -> RpcResult<GetDnsConfirmationResponse> {
        // kaspa-pq Phase 10 (ADR-0009): expose the current DnsState-derived
        // confirmation view. `available: false` when the overlay is not
        // configured for this network (or no DnsState has been written yet).
        let session = self.consensus_manager.consensus().unguarded_session();
        let confirmation = session.async_get_dns_confirmation().await;
        // Per-block finality is evaluated relative to the stable confirmed anchor; both
        // fields are `Copy`, so snapshot them before `confirmation` is consumed below.
        let anchor_info = confirmation.as_ref().map(|c| (c.last_dns_confirmed_anchor, c.dns_confirmed));
        let mut response = match confirmation {
            Some(c) => GetDnsConfirmationResponse {
                available: true,
                block_hash: c.block_hash.to_string(),
                work_depth: c.work_depth.to_string(),
                required_work_depth: c.required_work_depth.to_string(),
                stake_depth: c.stake_depth.to_string(),
                required_stake_depth: c.required_stake_depth.to_string(),
                pow_confirmed: c.pow_confirmed,
                dns_confirmed: c.dns_confirmed,
                rollout_stage: c.rollout_stage as u32,
                expected_dns_confirmation_seconds: c.expected_dns_confirmation_seconds,
                work_reorg_risk_upper_bound: c.work_reorg_risk_upper_bound,
                stake_reorg_risk_upper_bound: c.stake_reorg_risk_upper_bound,
                dns_reorg_risk_conservative_bound: c.dns_reorg_risk_conservative_bound,
                note: c.note,
                health: c.health as u32,
                // audit M-01: the stable DNS-confirmed anchor (≠ the pov-dependent sink `block_hash`).
                last_dns_confirmed_anchor: c.last_dns_confirmed_anchor.to_string(),
                last_dns_confirmed_anchor_daa_score: c.last_dns_confirmed_anchor_daa_score,
                // kaspa-pq explorer: per-block fields filled below when `request.block_hash` is set.
                block_found: false,
                block_is_dns_final: false,
                block_is_confirmed_anchor: false,
                block_daa_score: 0,
            },
            None => GetDnsConfirmationResponse::default(),
        };

        // kaspa-pq explorer support (`getBlockDnsScore`): when the caller supplies a specific
        // `block_hash`, answer "is THIS block DNS-final?" relative to the confirmed canonical
        // anchor. A block is DNS-final iff the overlay has confirmed an anchor and the block is
        // that anchor or one of its selected-chain ancestors (i.e. at/below the anchor on the
        // selected chain). Header-only blocks still count as `block_found`.
        if !request.block_hash.is_empty() {
            let hash = request
                .block_hash
                .parse::<kaspa_hashes::Hash64>()
                .map_err(|_| RpcError::General(format!("block_hash '{}' is not a valid 64-byte hash", request.block_hash)))?;
            if let Ok(block) = session.async_get_block_even_if_header_only(hash).await {
                response.block_found = true;
                response.block_daa_score = block.header.daa_score;
            }
            if let Some((anchor, dns_confirmed)) = anchor_info {
                if anchor != kaspa_hashes::Hash64::default() {
                    response.block_is_confirmed_anchor = hash == anchor;
                    response.block_is_dns_final = dns_confirmed
                        && (response.block_is_confirmed_anchor
                            || session.async_is_chain_ancestor_of(hash, anchor).await.unwrap_or(false));
                }
            }
        }

        Ok(response)
    }

    async fn submit_evm_transaction_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitEvmTransactionRequest,
    ) -> RpcResult<SubmitEvmTransactionResponse> {
        // kaspa-pq EVM Lane v0.4 (§16): hex → raw EIP-2718 bytes → EVM mempool.
        // Admission is the body-validation class-1 rule (non-evm builds refuse).
        // On success the tx is also queued for P2P relay to EVM-relay-capable
        // peers (§14.2), in addition to this node's own template payload.
        let hex_str = request.transaction.strip_prefix("0x").unwrap_or(&request.transaction);
        if hex_str.len() % 2 != 0 {
            return Err(RpcError::RpcSubsystem("odd-length transaction hex".to_string()));
        }
        let mut raw = vec![0u8; hex_str.len() / 2];
        faster_hex::hex_decode(hex_str.as_bytes(), &mut raw)
            .map_err(|e| RpcError::RpcSubsystem(format!("malformed transaction hex: {e}")))?;
        let hash = self
            .flow_context
            .submit_rpc_evm_transaction(raw)
            .await
            .map_err(|e| RpcError::RpcSubsystem(format!("evm mempool: {e}")))?;
        Ok(SubmitEvmTransactionResponse { transaction_hash: hash.to_string() })
    }

    async fn get_evm_transaction_receipt_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetEvmTransactionReceiptRequest,
    ) -> RpcResult<GetEvmTransactionReceiptResponse> {
        let tx_hash = parse_evm_tx_hash(&request.transaction_hash)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let view = session.async_get_evm_tx_receipt(tx_hash).await?;
        Ok(match view {
            None => GetEvmTransactionReceiptResponse::default(),
            Some(v) => GetEvmTransactionReceiptResponse {
                found: true,
                accepting_block: v.accepting_block.to_string(),
                evm_number: v.evm_number,
                receipt_index: v.receipt_index,
                succeeded: v.receipt.succeeded,
                gas_used: v.receipt.gas_used,
                cumulative_gas_used: v.receipt.cumulative_gas_used,
                logs: v
                    .receipt
                    .logs
                    .iter()
                    .map(|l| RpcEvmLog {
                        address: l.address.to_string(),
                        topics: l.topics.iter().map(|t| t.to_string()).collect(),
                        data: {
                            let mut hex = vec![0u8; l.data.len() * 2];
                            faster_hex::hex_encode(&l.data, &mut hex).expect("twice the input size");
                            // SAFETY: hex_encode writes ASCII hex only.
                            unsafe { String::from_utf8_unchecked(hex) }
                        },
                    })
                    .collect(),
            },
        })
    }

    async fn get_evm_tx_inclusion_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetEvmTxInclusionStatusRequest,
    ) -> RpcResult<GetEvmTxInclusionStatusResponse> {
        let tx_hash = parse_evm_tx_hash(&request.transaction_hash)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let row = session.async_get_evm_tx_locations(tx_hash).await?;
        // Canonical acceptance = the receipt view's resolution (§16: orphaned
        // acceptances read as not-accepted at `latest`).
        let receipt = session.async_get_evm_tx_receipt(tx_hash).await?;
        Ok(GetEvmTxInclusionStatusResponse {
            // §14/§18.1: the pre-inclusion tier — pending in this node's EVM
            // mempool (a tx can be both pending and included: inclusion does
            // not remove it from the pool under delayed acceptance).
            pending: self.mining_manager.has_pending_evm_transaction(&tx_hash),
            included_in: row.included_in.iter().map(|h| h.to_string()).collect(),
            accepted_in: receipt.as_ref().map(|v| v.accepting_block.to_string()).unwrap_or_default(),
            receipt_index: receipt.as_ref().map(|v| v.receipt_index).unwrap_or_default(),
            last_skip_class: if receipt.is_some() { 0 } else { row.last_skip_class.unwrap_or(0) as u32 },
        })
    }

    async fn submit_evm_deposit_claim_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitEvmDepositClaimRequest,
    ) -> RpcResult<SubmitEvmDepositClaimResponse> {
        // kaspa-pq EVM Lane v0.4 (§9.2): resolve the submitted EVM_DEPOSIT_LOCK
        // outpoint in the virtual UTXO set, read the locked fields, build +
        // validate a DepositClaim, and queue it for this node's own template.
        // The depositor knows their own outpoint, so this is a point lookup —
        // no scan, no index. The VSP template path re-validates against the
        // live selected-parent view before committing.
        let transaction_id: kaspa_hashes::Hash64 = request
            .transaction_id
            .strip_prefix("0x")
            .unwrap_or(&request.transaction_id)
            .parse()
            .map_err(|_| RpcError::RpcSubsystem("transaction_id must be a 64-byte hex hash".to_string()))?;
        let outpoint = kaspa_consensus_core::tx::TransactionOutpoint::new(transaction_id, request.index);

        let session = self.consensus_manager.consensus().unguarded_session();
        let entry = session
            .async_get_virtual_utxo_entry(outpoint)
            .await
            .ok_or_else(|| RpcError::RpcSubsystem(format!("outpoint {outpoint} is absent/spent in the virtual UTXO set")))?;
        let lock = kaspa_txscript::script_class::parse_evm_deposit_lock(&entry.script_public_key)
            .ok_or_else(|| RpcError::RpcSubsystem(format!("outpoint {outpoint} is not an EVM_DEPOSIT_LOCK output")))?;

        // audit #9: mirror the consensus rule (validate_evm_deposit_claims) so the
        // RPC rejects an unclaimable lock up front instead of "successfully" queueing
        // a claim the template path will silently drop. tip > amount is consensus-invalid.
        if lock.claim_tip_sompi > entry.amount {
            return Err(RpcError::RpcSubsystem(format!(
                "deposit lock {outpoint} is unclaimable: claim_tip {} exceeds locked amount {}",
                lock.claim_tip_sompi, entry.amount
            )));
        }

        // The claim mirrors the lock exactly (the consensus rule binds them).
        let claim = kaspa_consensus_core::evm::DepositClaim {
            deposit_outpoint: outpoint,
            evm_address: kaspa_consensus_core::evm::EvmAddress::from_bytes(lock.evm_address),
            amount_sompi: entry.amount,
            claim_tip_sompi: lock.claim_tip_sompi,
        };

        // Reject early if already in the refund window (AC-2 exclusivity): a
        // claim at/after the lock timeout is invalid, so do not queue it.
        let sink_daa = session.async_get_sink_daa_score_timestamp().await.daa_score;
        if sink_daa >= lock.timeout_daa_score {
            return Err(RpcError::RpcSubsystem(format!(
                "deposit lock {outpoint} is at/past its refund timeout {} (sink daa {sink_daa})",
                lock.timeout_daa_score
            )));
        }

        // §14.2: queue locally AND gossip the lock outpoint for P2P relay, so the
        // claim reaches the dominant selected-chain producer regardless of which
        // node the depositor submitted to (mirrors submit_evm_transaction_call).
        if !self.flow_context.submit_rpc_evm_deposit_claim(claim.clone()).await {
            return Err(RpcError::RpcSubsystem("the deposit-claim queue is full".to_string()));
        }
        let mut evm_address_hex = vec![0u8; 40];
        faster_hex::hex_encode(&lock.evm_address, &mut evm_address_hex)
            .map_err(|e| RpcError::RpcSubsystem(format!("hex encode: {e}")))?;
        Ok(SubmitEvmDepositClaimResponse {
            evm_address: format!("0x{}", String::from_utf8(evm_address_hex).unwrap()),
            amount_sompi: claim.amount_sompi.saturating_sub(claim.claim_tip_sompi),
            claim_tip_sompi: claim.claim_tip_sompi,
        })
    }

    async fn get_validator_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetValidatorStatusRequest,
    ) -> RpcResult<GetValidatorStatusResponse> {
        // kaspa-pq Phase 11 (ADR-0010): delegate to the in-process validator service when
        // present (`--enable-validator`); `enabled: false` otherwise.
        Ok(match &self.validator_status_provider {
            Some(provider) => provider.rpc_validator_status().await,
            None => GetValidatorStatusResponse::default(),
        })
    }

    async fn get_validator_attestation_target_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetValidatorAttestationTargetRequest,
    ) -> RpcResult<GetValidatorAttestationTargetResponse> {
        // kaspa-pq Phase 12 (ADR-0011): assemble the ready-to-sign attestation target for
        // `request.bond_outpoint` so the `kaspa-pq-validator` sidecar can fetch the signing
        // message over local wRPC. A malformed outpoint is a request error; `available: false`
        // when the overlay is not configured or no target could be assembled.
        let bond_outpoint = parse_bond_outpoint(&request.bond_outpoint)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        Ok(match session.async_get_validator_attestation_target(bond_outpoint).await {
            Some(t) => GetValidatorAttestationTargetResponse {
                available: true,
                epoch: t.epoch,
                target_hash: t.target_hash.to_string(),
                target_daa_score: t.target_daa_score,
                validator_set_commitment: t.validator_set_commitment.to_string(),
                message: t.message.to_string(),
            },
            None => GetValidatorAttestationTargetResponse::default(),
        })
    }

    async fn get_stake_bond_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetStakeBondRequest,
    ) -> RpcResult<GetStakeBondResponse> {
        // kaspa-pq Phase 12 (ADR-0011): the sidecar's own stake-bond status, evaluated at
        // the node's sink so it matches what the validator would attest for. A malformed
        // outpoint is a request error; `available: false` when the overlay is off or no
        // such bond exists.
        let bond_outpoint = parse_bond_outpoint(&request.bond_outpoint)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        Ok(match session.async_get_stake_bond(bond_outpoint).await {
            Some(r) => {
                let sink_daa = session.async_get_sink_daa_score_timestamp().await.daa_score;
                let effective = kaspa_consensus_core::dns_finality::effective_bond_status(&r, sink_daa);
                GetStakeBondResponse {
                    available: true,
                    validator_id: r.validator_pubkey_hash.to_string(),
                    amount: r.amount,
                    activation_daa_score: r.activation_daa_score,
                    effective_status: match effective {
                        kaspa_consensus_core::dns_finality::BondStatus::Pending => "pending",
                        kaspa_consensus_core::dns_finality::BondStatus::Active => "active",
                        kaspa_consensus_core::dns_finality::BondStatus::Unbonding => "unbonding",
                        kaspa_consensus_core::dns_finality::BondStatus::Slashed => "slashed",
                    }
                    .to_string(),
                }
            }
            None => GetStakeBondResponse::default(),
        })
    }

    async fn get_virtual_chain_from_block_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetVirtualChainFromBlockRequest,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        let session = self.consensus_manager.consensus().session().await;

        // This RPC call attempts to retrieve transactions on route from the block to the virtual
        // These transactions may not be present during a transitional state where the sink is missing a block body
        if session.async_is_consensus_in_transitional_ibd_state().await {
            return Err(RpcError::ConsensusInTransitionalIbdState);
        }
        // batch_size is set to 10 times the mergeset_size_limit.
        // this means batch_size is 2480 on 10 bps, and 1800 on mainnet.
        // this bounds by number of merged blocks, if include_accepted_transactions = true
        // else it returns the batch_size amount on pure chain blocks.
        // Note: batch_size does not bound removed chain blocks, only added chain blocks.
        let batch_size = (self.config.mergeset_size_limit() * 10) as usize;
        let mut virtual_chain_batch = session.async_get_virtual_chain_from_block(request.start_hash, Some(batch_size)).await?;

        if let Some(min_confirmation_count) = request.min_confirmation_count
            && min_confirmation_count > 0
        {
            let sink_blue_score = session.async_get_sink_blue_score().await;

            while !virtual_chain_batch.added.is_empty() {
                let vc_last_accepted_block_hash = virtual_chain_batch.added.last().unwrap();
                let vc_last_accepted_block = session.async_get_block(*vc_last_accepted_block_hash).await?;

                let distance = sink_blue_score.saturating_sub(vc_last_accepted_block.header.blue_score);

                if distance > min_confirmation_count {
                    break;
                }

                virtual_chain_batch.added.pop();
            }
        }

        let accepted_transaction_ids = if request.include_accepted_transaction_ids {
            let accepted_transaction_ids = self
                .consensus_converter
                .get_virtual_chain_accepted_transaction_ids(&session, &virtual_chain_batch, Some(batch_size))
                .await?;
            // bound added to the length of the accepted transaction ids, which is bounded by merged blocks
            virtual_chain_batch.added.truncate(accepted_transaction_ids.len());
            accepted_transaction_ids
        } else {
            vec![]
        };
        Ok(GetVirtualChainFromBlockResponse::new(virtual_chain_batch.removed, virtual_chain_batch.added, accepted_transaction_ids))
    }

    async fn get_block_count_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetBlockCountRequest,
    ) -> RpcResult<GetBlockCountResponse> {
        Ok(self.consensus_manager.consensus().unguarded_session().async_estimate_block_count().await)
    }

    async fn get_utxos_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetUtxosByAddressesRequest,
    ) -> RpcResult<GetUtxosByAddressesResponse> {
        if !self.config.utxoindex {
            return Err(RpcError::NoUtxoIndex);
        }
        let session = self.consensus_manager.consensus().unguarded_session();
        // do not retrieve utxos  while in unstable ibd state.
        if session.async_is_consensus_in_transitional_ibd_state().await {
            return Err(RpcError::ConsensusInTransitionalIbdState);
        }

        // TODO: discuss if the entry order is part of the method requirements
        //       (the current impl does not retain an entry order matching the request addresses order)
        //
        // NOTE (large-UTXO availability): this method is unbounded — it loads and serializes *every*
        // UTXO of the requested addresses (see the pagination TODO in
        // indexes/utxoindex/src/stores/indexed_utxos.rs). A heavily-fragmented address (e.g. a mining
        // payout that is never consolidated) can hold hundreds of thousands of UTXOs, producing a
        // response that exceeds the wRPC frame cap (MAX_WRPC_MESSAGE_SIZE = 128 MiB) or the client's
        // own message-size / request timeout, which surfaces to the caller as "connection closed".
        // The measurement below lets operators pinpoint such cases (entry count + scan/convert time +
        // an estimated serialized size) instead of guessing at the disconnect cause.
        let started = Instant::now();
        let num_addresses = request.addresses.len();
        let entry_map = self.get_utxo_set_by_script_public_key(request.addresses.iter()).await;
        let entries = self.index_converter.get_utxos_by_addresses_entries(&entry_map);
        let num_entries = entries.len();
        let elapsed = started.elapsed();
        // ~200 bytes/entry is a conservative borsh estimate (outpoint 36 + compact utxo entry +
        // 64-byte PubKeyHashMlDsa87 script + repeated address); JSON is ~2.5x larger.
        let est_borsh_mib = (num_entries.saturating_mul(200)) as f64 / (1024.0 * 1024.0);
        // Audit H-02: HARD CAP the legacy unbounded method. Past this the borsh
        // response (~200 B/entry) approaches/exceeds the 128 MiB wRPC frame cap and
        // the serialize+write would exhaust node memory/CPU/socket (the remote DoS
        // the auditor describes). Return an explicit error BEFORE serializing and
        // steer the caller to the paginated getUtxosByAddressPage or the
        // balance-only getBalancesByAddresses, instead of silently continuing.
        const LARGE_UTXO_HARD_CAP: usize = 250_000;
        if num_entries > LARGE_UTXO_HARD_CAP {
            return Err(RpcError::General(format!(
                "getUtxosByAddresses: {num_entries} UTXOs across {num_addresses} address(es) exceeds the {LARGE_UTXO_HARD_CAP} hard cap \
(~{est_borsh_mib:.0} MiB); use getUtxosByAddressPage (cursor-paginated) or getBalancesByAddresses (balance only)."
            )));
        }
        const LARGE_UTXO_RESPONSE_THRESHOLD: usize = 50_000;
        if num_entries >= LARGE_UTXO_RESPONSE_THRESHOLD {
            let msg = format!(
                "get_utxos_by_addresses: large response — {} UTXOs across {} address(es) in {:.0}ms (~{:.0} MiB borsh est, ~{:.0} MiB JSON est); \
may exceed the client's message-size/timeout limit, or the 128 MiB wRPC frame cap for very large sets (\"connection closed\"). \
Use getBalancesByAddresses for balances, or consolidate the address's UTXOs.",
                num_entries,
                num_addresses,
                elapsed.as_secs_f64() * 1000.0,
                est_borsh_mib,
                est_borsh_mib * 2.5,
            );
            // Rate-limit the WARN: explorers/wallets poll on a cadence, so a persistently-fragmented
            // address would otherwise log on every call. Emit at WARN at most once per 60s
            // (process-wide); in between, the same detail stays available at debug level.
            use std::sync::atomic::{AtomicU64, Ordering};
            static LAST_WARN_UNIX_SECS: AtomicU64 = AtomicU64::new(0);
            let now_secs = unix_now() / 1000;
            let last = LAST_WARN_UNIX_SECS.load(Ordering::Relaxed);
            let do_warn = now_secs.saturating_sub(last) >= 60
                && LAST_WARN_UNIX_SECS.compare_exchange(last, now_secs, Ordering::Relaxed, Ordering::Relaxed).is_ok();
            if do_warn {
                warn!("{msg}");
            } else {
                debug!("{msg} (warn rate-limited)");
            }
        } else {
            debug!(
                "get_utxos_by_addresses: {} UTXOs across {} address(es) in {:.0}ms (~{:.1} MiB borsh est)",
                num_entries,
                num_addresses,
                elapsed.as_secs_f64() * 1000.0,
                est_borsh_mib,
            );
        }
        Ok(GetUtxosByAddressesResponse::new(entries))
    }

    async fn get_utxos_by_address_page_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetUtxosByAddressPageRequest,
    ) -> RpcResult<GetUtxosByAddressPageResponse> {
        if !self.config.utxoindex {
            return Err(RpcError::NoUtxoIndex);
        }
        let session = self.consensus_manager.consensus().unguarded_session();
        if session.async_is_consensus_in_transitional_ibd_state().await {
            return Err(RpcError::ConsensusInTransitionalIbdState);
        }
        // Bound the page so a single call can never reproduce the unbounded getUtxosByAddresses blow-up.
        const DEFAULT_PAGE_LIMIT: u64 = 1_000;
        const MAX_PAGE_LIMIT: u64 = 1_000;
        let limit = if request.limit == 0 { DEFAULT_PAGE_LIMIT } else { request.limit.min(MAX_PAGE_LIMIT) } as usize;
        // The cursor is an opaque hex token (the previous page's resume key). A malformed token is
        // treated leniently as "no cursor" (restart from the beginning) rather than an error.
        let cursor = if request.cursor.is_empty() {
            None
        } else {
            let c = request.cursor.as_str();
            if c.len() % 2 == 0 {
                (0..c.len()).step_by(2).map(|i| u8::from_str_radix(&c[i..i + 2], 16).ok()).collect::<Option<Vec<u8>>>()
            } else {
                None
            }
        };
        let script_public_key = pay_to_address_script(&request.address);
        let (entry_map, next) = self
            .utxoindex
            .clone()
            .ok_or(RpcError::NoUtxoIndex)?
            .get_utxos_by_script_public_key_chunk(script_public_key, cursor, limit)
            .await
            .map_err(|err| RpcError::General(err.to_string()))?;
        let entries = self.index_converter.get_utxos_by_addresses_entries(&entry_map);
        let next_cursor = next
            .map(|bytes| {
                use std::fmt::Write;
                let mut s = String::with_capacity(bytes.len() * 2);
                for b in &bytes {
                    let _ = write!(s, "{b:02x}");
                }
                s
            })
            .unwrap_or_default();
        Ok(GetUtxosByAddressPageResponse::new(entries, next_cursor))
    }

    async fn get_balance_by_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetBalanceByAddressRequest,
    ) -> RpcResult<GetBalanceByAddressResponse> {
        if !self.config.utxoindex {
            return Err(RpcError::NoUtxoIndex);
        }

        let session = self.consensus_manager.consensus().unguarded_session();

        // do not retrieve utxo balances while in unstable ibd state.
        if session.async_is_consensus_in_transitional_ibd_state().await {
            return Err(RpcError::ConsensusInTransitionalIbdState);
        }
        let entry_map = self.get_balance_by_script_public_key(once(&request.address)).await;
        let balance = entry_map.values().sum();
        Ok(GetBalanceByAddressResponse::new(balance))
    }

    async fn get_balances_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse> {
        if !self.config.utxoindex {
            return Err(RpcError::NoUtxoIndex);
        }
        let session = self.consensus_manager.consensus().unguarded_session();

        // do not retrieve utxo balances while in unstable ibd state.
        if session.async_is_consensus_in_transitional_ibd_state().await {
            return Err(RpcError::ConsensusInTransitionalIbdState);
        }
        let entry_map = self.get_balance_by_script_public_key(request.addresses.iter()).await;
        let entries = request
            .addresses
            .iter()
            .map(|address| {
                let script_public_key = pay_to_address_script(address);
                let balance = entry_map.get(&script_public_key).copied();
                RpcBalancesByAddressesEntry { address: address.to_owned(), balance }
            })
            .collect();
        Ok(GetBalancesByAddressesResponse::new(entries))
    }

    async fn get_coin_supply_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetCoinSupplyRequest,
    ) -> RpcResult<GetCoinSupplyResponse> {
        if !self.config.utxoindex {
            return Err(RpcError::NoUtxoIndex);
        }
        let session = self.consensus_manager.consensus().unguarded_session();

        // do not retrieve supply balances while in unstable ibd state.
        if session.async_is_consensus_in_transitional_ibd_state().await {
            return Err(RpcError::ConsensusInTransitionalIbdState);
        }
        let circulating_sompi =
            self.utxoindex.clone().unwrap().get_circulating_supply().await.map_err(|e| RpcError::General(e.to_string()))?;
        Ok(GetCoinSupplyResponse::new(MAX_SOMPI, circulating_sompi))
    }

    async fn get_daa_score_timestamp_estimate_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetDaaScoreTimestampEstimateRequest,
    ) -> RpcResult<GetDaaScoreTimestampEstimateResponse> {
        let session = self.consensus_manager.consensus().session().await;
        // TODO: cache samples based on sufficient recency of the data and append sink data
        let mut headers = session.async_get_chain_block_samples().await;
        let mut requested_daa_scores = request.daa_scores.clone();
        let mut daa_score_timestamp_map = HashMap::<u64, u64>::new();

        headers.reverse();
        requested_daa_scores.sort_by(|a, b| b.cmp(a));

        let mut header_idx = 0;
        let mut req_idx = 0;

        // TODO (relaxed; post-HF): the below interpolation should remain valid also after the hardfork as long
        // as the two pruning points used are both either from before activation or after. The only exception are
        // the two pruning points before and after activation. However this inaccuracy can be considered negligible.
        // Alternatively, we can remedy this post the HF by manually adding a (DAA score, timestamp) point from the
        // moment of activation.

        // Loop runs at O(n + m) where n = # pp headers, m = # requested daa_scores
        // Loop will always end because in the worst case the last header with daa_score = 0 (the genesis)
        // will cause every remaining requested daa_score to be "found in range"
        //
        // TODO: optimize using binary search over the samples to obtain O(m log n) complexity (which is an improvement assuming m << n)
        while header_idx < headers.len() && req_idx < request.daa_scores.len() {
            let header = headers.get(header_idx).unwrap();
            let curr_daa_score = requested_daa_scores[req_idx];

            // Found daa_score in range
            if header.daa_score <= curr_daa_score {
                // For daa_score later than the last header, we estimate in milliseconds based on the difference
                let time_adjustment = if header_idx == 0 {
                    // estimate milliseconds = (daa_score * target_time_per_block)
                    (curr_daa_score - header.daa_score).saturating_mul(self.config.target_time_per_block())
                } else {
                    // "next" header is the one that we processed last iteration
                    let next_header = &headers[header_idx - 1];
                    // Unlike DAA scores which are monotonic (over the selected chain), timestamps are not strictly monotonic, so we avoid assuming so
                    let time_between_headers = next_header.timestamp.saturating_sub(header.timestamp);
                    let score_between_query_and_header = (curr_daa_score - header.daa_score) as f64;
                    let score_between_headers = (next_header.daa_score - header.daa_score) as f64;
                    // Interpolate the timestamp delta using the estimated fraction based on DAA scores
                    ((time_between_headers as f64) * (score_between_query_and_header / score_between_headers)) as u64
                };

                let daa_score_timestamp = header.timestamp.saturating_add(time_adjustment);
                daa_score_timestamp_map.insert(curr_daa_score, daa_score_timestamp);

                // Process the next daa score that's <= than current one (at earlier idx)
                req_idx += 1;
            } else {
                header_idx += 1;
            }
        }

        // Note: it is safe to assume all entries exist in the map since the first sampled header is expected to have daa_score=0
        let timestamps = request.daa_scores.iter().map(|curr_daa_score| daa_score_timestamp_map[curr_daa_score]).collect();

        Ok(GetDaaScoreTimestampEstimateResponse::new(timestamps))
    }

    async fn get_fee_estimate_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetFeeEstimateRequest,
    ) -> RpcResult<GetFeeEstimateResponse> {
        let mining_manager = self.mining_manager.clone();
        let estimate =
            self.fee_estimate_cache.get(async move { mining_manager.get_realtime_feerate_estimations().await.into_rpc() }).await;
        Ok(GetFeeEstimateResponse { estimate })
    }

    async fn get_fee_estimate_experimental_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetFeeEstimateExperimentalRequest,
    ) -> RpcResult<GetFeeEstimateExperimentalResponse> {
        if request.verbose {
            let mining_manager = self.mining_manager.clone();
            let consensus_manager = self.consensus_manager.clone();
            let prefix = self.config.prefix();

            let response = self
                .fee_estimate_verbose_cache
                .get(async move {
                    let session = consensus_manager.consensus().unguarded_session();
                    mining_manager.get_realtime_feerate_estimations_verbose(&session, prefix).await.map(FeeEstimateVerbose::into_rpc)
                })
                .await?;
            Ok(response)
        } else {
            let estimate = self.get_fee_estimate_call(connection, GetFeeEstimateRequest {}).await?.estimate;
            Ok(GetFeeEstimateExperimentalResponse { estimate, verbose: None })
        }
    }

    async fn get_utxo_return_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetUtxoReturnAddressRequest,
    ) -> RpcResult<GetUtxoReturnAddressResponse> {
        let session = self.consensus_manager.consensus().session().await;

        // do not retrieve utxos while in unstable ibd state.
        if session.async_is_consensus_in_transitional_ibd_state().await {
            return Err(RpcError::ConsensusInTransitionalIbdState);
        }

        match session
            .async_get_transactions_by_accepting_daa_score(
                request.accepting_block_daa_score,
                Some(vec![request.txid]),
                TransactionType::SignableTransaction,
            )
            .await?
        {
            TransactionQueryResult::SignableTransaction(txs) => {
                if txs.is_empty() {
                    return Err(RpcError::ConsensusError(UtxoInquirerError::TransactionNotFound.into()));
                };

                if txs[0].tx.inputs.is_empty() || txs[0].entries.is_empty() {
                    return Err(RpcError::ConsensusError(UtxoInquirerError::TxFromCoinbase.into()));
                }

                if let Some(utxo_entry) = &txs[0].entries[0] {
                    if let Ok(address) = extract_script_pub_key_address(&utxo_entry.script_public_key, self.config.prefix()) {
                        Ok(GetUtxoReturnAddressResponse { return_address: address })
                    } else {
                        Err(RpcError::ConsensusError(UtxoInquirerError::NonStandard.into()))
                    }
                } else {
                    Err(RpcError::ConsensusError(UtxoInquirerError::UnfilledUtxoEntry.into()))
                }
            }
            TransactionQueryResult::Transaction(_) => Err(RpcError::ConsensusError(UtxoInquirerError::TransactionNotFound.into())),
        }
    }

    async fn ping_call(&self, _connection: Option<&DynRpcConnection>, _: PingRequest) -> RpcResult<PingResponse> {
        Ok(PingResponse {})
    }

    async fn get_headers_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetHeadersRequest,
    ) -> RpcResult<GetHeadersResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_dag_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetBlockDagInfoRequest,
    ) -> RpcResult<GetBlockDagInfoResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let (consensus_stats, tips, pruning_point, sink) =
            join!(session.async_get_stats(), session.async_get_tips(), session.async_pruning_point(), session.async_get_sink());
        Ok(GetBlockDagInfoResponse::new(
            self.config.net,
            consensus_stats.block_counts.block_count,
            consensus_stats.block_counts.header_count,
            tips,
            self.consensus_converter.get_difficulty_ratio(consensus_stats.virtual_stats.bits),
            consensus_stats.virtual_stats.past_median_time,
            session.get_virtual_parents().into_iter().collect::<Vec<_>>(),
            pruning_point,
            consensus_stats.virtual_stats.daa_score,
            sink,
        ))
    }

    async fn estimate_network_hashes_per_second_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse> {
        if !self.config.unsafe_rpc && request.window_size > MAX_SAFE_WINDOW_SIZE {
            return Err(RpcError::WindowSizeExceedingMaximum(request.window_size, MAX_SAFE_WINDOW_SIZE));
        }
        if request.window_size as u64 > self.config.pruning_depth() {
            return Err(RpcError::WindowSizeExceedingPruningDepth(request.window_size, self.config.pruning_depth()));
        }

        // In the previous golang implementation the convention for virtual was the following const.
        // In the current implementation, consensus behaves the same when it gets a None instead.
        // PR-9.5e: block hashes widened to Hash64; the legacy-virtual sentinel is a 64-byte all-0xff block id.
        const LEGACY_VIRTUAL: kaspa_consensus_core::BlockHash =
            kaspa_consensus_core::BlockHash::from_bytes([0xff; kaspa_hashes::HASH64_SIZE]);
        let mut start_hash = request.start_hash;
        if let Some(start) = start_hash
            && start == LEGACY_VIRTUAL
        {
            start_hash = None;
        }

        Ok(EstimateNetworkHashesPerSecondResponse::new(
            self.consensus_manager
                .consensus()
                .session()
                .await
                .async_estimate_network_hashes_per_second(start_hash, request.window_size as usize)
                .await?,
        ))
    }

    async fn add_peer_call(&self, _connection: Option<&DynRpcConnection>, request: AddPeerRequest) -> RpcResult<AddPeerResponse> {
        if !self.config.unsafe_rpc {
            warn!("AddPeer RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }
        let peer_address = request.peer_address.normalize(self.config.net.default_p2p_port());
        if let Some(connection_manager) = self.flow_context.connection_manager() {
            connection_manager.add_connection_request(peer_address.into(), request.is_permanent).await;
        } else {
            return Err(RpcError::NoConnectionManager);
        }
        Ok(AddPeerResponse {})
    }

    async fn get_peer_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetPeerAddressesRequest,
    ) -> RpcResult<GetPeerAddressesResponse> {
        let address_manager = self.flow_context.address_manager.lock();
        Ok(GetPeerAddressesResponse::new(address_manager.get_all_addresses(), address_manager.get_all_banned_addresses()))
    }

    async fn ban_call(&self, _connection: Option<&DynRpcConnection>, request: BanRequest) -> RpcResult<BanResponse> {
        if !self.config.unsafe_rpc {
            warn!("Ban RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }
        if let Some(connection_manager) = self.flow_context.connection_manager() {
            let ip = request.ip.into();
            if connection_manager.ip_has_permanent_connection(ip).await {
                return Err(RpcError::IpHasPermanentConnection(request.ip));
            }
            connection_manager.ban(ip).await;
        } else {
            return Err(RpcError::NoConnectionManager);
        }
        Ok(BanResponse {})
    }

    async fn unban_call(&self, _connection: Option<&DynRpcConnection>, request: UnbanRequest) -> RpcResult<UnbanResponse> {
        if !self.config.unsafe_rpc {
            warn!("Unban RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }
        let mut address_manager = self.flow_context.address_manager.lock();
        if address_manager.is_banned(request.ip) {
            address_manager.unban(request.ip)
        } else {
            return Err(RpcError::IpIsNotBanned(request.ip));
        }
        Ok(UnbanResponse {})
    }

    async fn get_connected_peer_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetConnectedPeerInfoRequest,
    ) -> RpcResult<GetConnectedPeerInfoResponse> {
        let peers = self.flow_context.hub().active_peers();
        let peer_info = self.protocol_converter.get_peers_info(&peers);
        Ok(GetConnectedPeerInfoResponse::new(peer_info))
    }

    async fn shutdown_call(&self, _connection: Option<&DynRpcConnection>, _: ShutdownRequest) -> RpcResult<ShutdownResponse> {
        if !self.config.unsafe_rpc {
            warn!("Shutdown RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }
        warn!("Shutdown RPC command was called, shutting down in 1 second...");

        // Signal the shutdown request
        self.core_shutdown_request.trigger.trigger();

        // Wait for a second before shutting down,
        // giving time for the response to be sent to the caller.
        let core = self.core.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            core.shutdown();
        });

        Ok(ShutdownResponse {})
    }

    async fn resolve_finality_conflict_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse> {
        // TODO(Relaxed): implement this functionality
        // When implementing, make sure to consider transitional IBD state
        if !self.config.unsafe_rpc {
            warn!("ResolveFinalityConflict RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }
        Err(RpcError::NotImplemented)
    }

    async fn get_connections_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        req: GetConnectionsRequest,
    ) -> RpcResult<GetConnectionsResponse> {
        let clients = (self.wrpc_borsh_counters.active_connections.load(Ordering::Relaxed)
            + self.wrpc_json_counters.active_connections.load(Ordering::Relaxed)) as u32;
        let peers = self.flow_context.hub().active_peers_len() as u16;

        let profile_data = req.include_profile_data.then(|| {
            let CountersSnapshot { resident_set_size: memory_usage, cpu_usage, .. } = self.perf_monitor.snapshot();

            ConnectionsProfileData { cpu_usage: cpu_usage as f32, memory_usage }
        });

        Ok(GetConnectionsResponse { clients, peers, profile_data })
    }

    async fn get_metrics_call(&self, _connection: Option<&DynRpcConnection>, req: GetMetricsRequest) -> RpcResult<GetMetricsResponse> {
        let CountersSnapshot {
            resident_set_size,
            virtual_memory_size,
            core_num,
            cpu_usage,
            fd_num,
            disk_io_read_bytes,
            disk_io_write_bytes,
            disk_io_read_per_sec,
            disk_io_write_per_sec,
        } = self.perf_monitor.snapshot();

        let process_metrics = req.process_metrics.then_some(ProcessMetrics {
            resident_set_size,
            virtual_memory_size,
            core_num: core_num as u32,
            cpu_usage: cpu_usage as f32,
            fd_num: fd_num as u32,
            disk_io_read_bytes,
            disk_io_write_bytes,
            disk_io_read_per_sec: disk_io_read_per_sec as f32,
            disk_io_write_per_sec: disk_io_write_per_sec as f32,
        });

        let connection_metrics = req.connection_metrics.then(|| ConnectionMetrics {
            borsh_live_connections: self.wrpc_borsh_counters.active_connections.load(Ordering::Relaxed) as u32,
            borsh_connection_attempts: self.wrpc_borsh_counters.total_connections.load(Ordering::Relaxed) as u64,
            borsh_handshake_failures: self.wrpc_borsh_counters.handshake_failures.load(Ordering::Relaxed) as u64,
            json_live_connections: self.wrpc_json_counters.active_connections.load(Ordering::Relaxed) as u32,
            json_connection_attempts: self.wrpc_json_counters.total_connections.load(Ordering::Relaxed) as u64,
            json_handshake_failures: self.wrpc_json_counters.handshake_failures.load(Ordering::Relaxed) as u64,

            active_peers: self.flow_context.hub().active_peers_len() as u32,
        });

        let bandwidth_metrics = req.bandwidth_metrics.then(|| BandwidthMetrics {
            borsh_bytes_tx: self.wrpc_borsh_counters.tx_bytes.load(Ordering::Relaxed) as u64,
            borsh_bytes_rx: self.wrpc_borsh_counters.rx_bytes.load(Ordering::Relaxed) as u64,
            json_bytes_tx: self.wrpc_json_counters.tx_bytes.load(Ordering::Relaxed) as u64,
            json_bytes_rx: self.wrpc_json_counters.rx_bytes.load(Ordering::Relaxed) as u64,
            p2p_bytes_tx: self.p2p_tower_counters.bytes_tx.load(Ordering::Relaxed) as u64,
            p2p_bytes_rx: self.p2p_tower_counters.bytes_rx.load(Ordering::Relaxed) as u64,
            grpc_bytes_tx: self.grpc_tower_counters.bytes_tx.load(Ordering::Relaxed) as u64,
            grpc_bytes_rx: self.grpc_tower_counters.bytes_rx.load(Ordering::Relaxed) as u64,
        });

        let consensus_metrics = if req.consensus_metrics {
            let consensus_stats = self.consensus_manager.consensus().unguarded_session().async_get_stats().await;
            let processing_counters = self.processing_counters.snapshot();

            Some(ConsensusMetrics {
                node_blocks_submitted_count: processing_counters.blocks_submitted,
                node_headers_processed_count: processing_counters.header_counts,
                node_dependencies_processed_count: processing_counters.dep_counts,
                node_bodies_processed_count: processing_counters.body_counts,
                node_transactions_processed_count: processing_counters.txs_counts,
                node_chain_blocks_processed_count: processing_counters.chain_block_counts,
                node_mass_processed_count: processing_counters.mass_counts,
                // ---
                node_database_blocks_count: consensus_stats.block_counts.block_count,
                node_database_headers_count: consensus_stats.block_counts.header_count,
                // ---
                network_mempool_size: self.mining_manager.transaction_count_sample(TransactionQuery::TransactionsOnly),
                network_tip_hashes_count: consensus_stats.num_tips.try_into().unwrap_or(u32::MAX),
                network_difficulty: self.consensus_converter.get_difficulty_ratio(consensus_stats.virtual_stats.bits),
                network_past_median_time: consensus_stats.virtual_stats.past_median_time,
                network_virtual_parent_hashes_count: consensus_stats.virtual_stats.num_parents,
                network_virtual_daa_score: consensus_stats.virtual_stats.daa_score,
            })
        } else {
            None
        };

        let storage_metrics = req.storage_metrics.then_some(StorageMetrics { storage_size_bytes: 0 });

        let custom_metrics: Option<HashMap<String, CustomMetricValue>> = None;

        let server_time = unix_now();

        let response = GetMetricsResponse {
            server_time,
            process_metrics,
            connection_metrics,
            bandwidth_metrics,
            consensus_metrics,
            storage_metrics,
            custom_metrics,
        };

        Ok(response)
    }

    async fn get_system_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSystemInfoRequest,
    ) -> RpcResult<GetSystemInfoResponse> {
        let response = GetSystemInfoResponse {
            version: self.system_info.version.clone(),
            system_id: self.system_info.system_id.clone(),
            git_hash: self.system_info.git_short_hash.clone(),
            cpu_physical_cores: self.system_info.cpu_physical_cores,
            total_memory: self.system_info.total_memory,
            fd_limit: self.system_info.fd_limit,
            proxy_socket_limit_per_cpu_core: self.system_info.proxy_socket_limit_per_cpu_core,
        };

        Ok(response)
    }

    async fn get_server_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetServerInfoRequest,
    ) -> RpcResult<GetServerInfoResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let sink_daa_score_timestamp = session.async_get_sink_daa_score_timestamp().await;
        let is_synced = self.mining_rule_engine.is_sink_recent_and_connected(sink_daa_score_timestamp);
        let virtual_daa_score = session.get_virtual_daa_score();

        Ok(GetServerInfoResponse {
            rpc_api_version: RPC_API_VERSION,
            rpc_api_revision: RPC_API_REVISION,
            server_version: version().to_string(),
            network_id: self.config.net,
            has_utxo_index: self.config.utxoindex,
            is_synced,
            virtual_daa_score,
        })
    }

    async fn get_sync_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSyncStatusRequest,
    ) -> RpcResult<GetSyncStatusResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();

        let sink_daa_score_timestamp = session.async_get_sink_daa_score_timestamp().await;
        let is_synced = self.mining_rule_engine.is_sink_recent_and_connected(sink_daa_score_timestamp)
            && !session.async_is_consensus_in_transitional_ibd_state().await;
        Ok(GetSyncStatusResponse { is_synced })
    }

    async fn get_virtual_chain_from_block_v2_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetVirtualChainFromBlockV2Request,
    ) -> RpcResult<GetVirtualChainFromBlockV2Response> {
        let session = self.consensus_manager.consensus().session().await;
        // sets to full by default
        let data_verbosity_level = request.data_verbosity_level.or(Some(RpcDataVerbosityLevel::Full));
        let verbosity: RpcAcceptanceDataVerbosity = data_verbosity_level.map(RpcAcceptanceDataVerbosity::from).unwrap_or_default();
        let batch_size = (self.config.mergeset_size_limit() * 10) as usize;

        let mut chain_path = session.async_get_virtual_chain_from_block(request.start_hash, Some(batch_size)).await?;

        // if min confirmation count is present, strip chain head if needed
        // so the new head has at least min_confirmation_count confirmations
        if let Some(min_confirmation_count) = request.min_confirmation_count
            && min_confirmation_count > 0
        {
            let sink_blue_score = session.async_get_sink_blue_score().await;

            while !chain_path.added.is_empty() {
                let vc_last_accepted_block_hash = chain_path.added.last().unwrap();
                let vc_last_accepted_block = session.async_get_block(*vc_last_accepted_block_hash).await?;

                let distance = sink_blue_score.saturating_sub(vc_last_accepted_block.header.blue_score);

                if distance > min_confirmation_count {
                    break;
                }

                chain_path.added.pop();
            }
        }

        let chain_blocks_accepted_transactions = self
            .consensus_converter
            .get_chain_blocks_accepted_transactions(&session, &verbosity, &chain_path, Some(batch_size))
            .await?;

        chain_path.added.truncate(chain_blocks_accepted_transactions.len());

        Ok(GetVirtualChainFromBlockV2Response {
            removed_chain_block_hashes: chain_path.removed.into(),
            added_chain_block_hashes: chain_path.added.into(),
            chain_block_accepted_transactions: chain_blocks_accepted_transactions.into(),
        })
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id identifying it.
    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId {
        self.notifier.register_new_listener(connection, ListenerLifespan::Dynamic)
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener, unregister the id and its associated connection.
    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        self.notifier.unregister_listener(id)?;
        Ok(())
    }

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        match scope {
            Scope::UtxosChanged(ref utxos_changed_scope) if !self.config.unsafe_rpc && utxos_changed_scope.addresses.is_empty() => {
                // The subscription to blanket UtxosChanged notifications is restricted to unsafe mode only
                // since the notifications yielded are highly resource intensive.
                //
                // Please note that unsubscribing to blanket UtxosChanged is always allowed and cancels
                // the whole subscription no matter if blanket or targeting specified addresses.

                warn!("RPC subscription to blanket UtxosChanged called while node in safe RPC mode -- ignoring.");
                Err(RpcError::UnavailableInSafeMode)
            }
            _ => {
                self.notifier.clone().start_notify(id, scope).await?;
                Ok(())
            }
        }
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.notifier.clone().stop_notify(id, scope).await?;
        Ok(())
    }
}

// It might be necessary to opt this out in the context of wasm32

impl AsyncService for RpcCoreService {
    fn ident(self: Arc<Self>) -> &'static str {
        Self::IDENT
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", Self::IDENT);
        let service = self.clone();

        // Prepare a shutdown signal receiver
        let shutdown_signal = self.shutdown.listener.clone();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            service.clone().start_impl();
            shutdown_signal.await;
            match service.join().await {
                Ok(_) => Ok(()),
                Err(err) => {
                    warn!("Error while stopping {}: {}", Self::IDENT, err);
                    Err(AsyncServiceError::Service(err.to_string()))
                }
            }
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", Self::IDENT);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", Self::IDENT);
            Ok(())
        })
    }
}
