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
use kaspa_consensusmanager::{ConsensusManager, ConsensusSessionOwned};
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

/// **What this node's build knows about a PALW class** — its model and the context its canonical
/// job runs at. `getPalwClassContexts` asks it for a class whose declaration this node did not index
/// (a genesis class carries none), and for the model id of one it did. Defined here, beside the
/// node crate's other bridge, so the build's ledger can be handed in without a circular dependency;
/// a node constructed without one answers from the chain alone.
pub trait PalwClassLedgerProvider: Send + Sync {
    fn class_context(&self, class_id: kaspa_hashes::Hash64) -> Option<PalwClassLedgerContext>;

    /// ADR-0131: the economic compute of the class's job as this build derives it from the class's
    /// own graph — the draw job (ADR-0117's, without decode calls) and the canonical job. `None` for
    /// a class the build does not supply; a build that has not derived it answers nothing.
    fn class_economic_compute(&self, class_id: kaspa_hashes::Hash64) -> Option<PalwClassLedgerCompute> {
        let _ = class_id;
        None
    }

    /// ADR-0132: bring the node's end-to-end ledger up to the chain before a read. Blocking (it
    /// reads consensus and writes the node's database); a node without a recorder does nothing.
    fn refresh_economics_ledger(&self) {}

    /// ADR-0132: how many claims the ledger holds and the accepted-DAA span they cover.
    fn economics_ledger_summary(&self) -> Option<PalwEconomicsLedgerSummary> {
        None
    }

    /// ADR-0132: the ledger's totals for one class.
    fn class_ledger_totals(
        &self,
        class_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::palw_economics_ledger_v1::PalwClassLedgerTotalsV1> {
        let _ = class_id;
        None
    }

    /// ADR-0132: what this node itself did for the class since the process started.
    fn class_node_telemetry(&self, class_id: kaspa_hashes::Hash64) -> Option<PalwClassNodeTelemetry> {
        let _ = class_id;
        None
    }
}

/// ADR-0132: the ledger's own span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwEconomicsLedgerSummary {
    pub claims: u64,
    pub first_daa: u64,
    pub last_daa: u64,
}

/// ADR-0132: what one node did for one class — its producer's draws and its seat's replays and
/// receipts, counted since the process started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClassNodeTelemetry {
    pub draws: u64,
    pub class_wins: u64,
    pub produced: u64,
    pub draw_millis: u64,
    pub storage_read_mib: u64,
    pub replays: u64,
    pub replay_millis: u64,
    pub replay_leaves: u64,
    pub receipts_valid: u64,
    pub receipts_unavailable: u64,
    pub receipts_incapable: u64,
    pub receipts_other: u64,
    pub openings_held: u64,
}

/// [`RpcPalwClassLedgerTotals`] of a class's ledger totals, every `u128` as a decimal string.
fn rpc_palw_class_ledger_totals(
    t: &kaspa_consensus_core::palw_economics_ledger_v1::PalwClassLedgerTotalsV1,
) -> RpcPalwClassLedgerTotals {
    RpcPalwClassLedgerTotals {
        available: true,
        claims: t.claims,
        bound: t.bound,
        licensed: t.licensed,
        finals: t.finals,
        voided: t.voided,
        redrawn: t.redrawn,
        paid_at_acceptance: t.paid_at_acceptance,
        escrow_final_sompi: t.escrow_final_sompi.to_string(),
        producer_paid_sompi: t.producer_paid_sompi.to_string(),
        panel_paid_sompi: t.panel_paid_sompi.to_string(),
        reserve_sompi: t.reserve_sompi.to_string(),
        burned_sompi: t.burned_sompi.to_string(),
        attempted_compute: t.attempted_compute.to_string(),
        final_compute: t.final_compute.to_string(),
        verification_compute: t.verification_compute.to_string(),
        producer_per_attempted_compute: t.producer_per_attempted_compute().to_string(),
        panel_per_verification_compute: t.panel_per_verification_compute().to_string(),
        total_per_attempted_compute: t.total_per_attempted_compute().to_string(),
        total_per_final_compute: t.total_per_final_compute().to_string(),
        licence_rate_permille: t.licence_rate_permille(),
        final_of_licensed_permille: t.final_of_licensed_permille(),
        final_rate_permille: t.final_rate_permille(),
        avg_bind_wait_daa: t.avg_bind_wait_daa(),
        avg_licence_wait_daa: t.avg_licence_wait_daa(),
        avg_final_wait_daa: t.avg_final_wait_daa(),
        avg_void_wait_daa: t.avg_void_wait_daa(),
        avg_expected_attempts_q32: t.avg_expected_attempts_q32().to_string(),
        avg_network_expected_attempts_q32: t.avg_network_expected_attempts_q32().to_string(),
        first_accepted_daa: t.first_accepted_daa,
        last_accepted_daa: t.last_accepted_daa,
    }
}

fn rpc_palw_class_node_telemetry(t: PalwClassNodeTelemetry) -> RpcPalwClassNodeTelemetry {
    RpcPalwClassNodeTelemetry {
        available: true,
        draws: t.draws,
        class_wins: t.class_wins,
        produced: t.produced,
        draw_millis: t.draw_millis,
        storage_read_mib: t.storage_read_mib,
        replays: t.replays,
        replay_millis: t.replay_millis,
        replay_leaves: t.replay_leaves,
        receipts_valid: t.receipts_valid,
        receipts_unavailable: t.receipts_unavailable,
        receipts_incapable: t.receipts_incapable,
        receipts_other: t.receipts_other,
        openings_held: t.openings_held,
    }
}

/// A class's economic compute as a build ledger derives it (ADR-0131 `EconomicComputeV1`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClassLedgerCompute {
    /// The job an attempt runs past `palw_prefill_draw`: the canonical job without decode calls.
    pub draw: u128,
    /// The canonical job, decode calls included — the job an attempt runs below that fence.
    pub canonical: u128,
}

/// One class as a build ledger records it. No footprint: the service derives it from the prefill and
/// the decode, by the rule consensus applies, for the ledger's rows and the chain's alike.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwClassLedgerContext {
    pub model_id: String,
    pub n_ctx: u32,
    pub canonical_prefill_tokens: u32,
    pub canonical_decode_tokens: u32,
    pub max_context_tokens: u32,
}

/// The numbers a class declaration the chain registered fixes: its graph's `n_ctx` and its canonical
/// job's token counts.
struct PalwRegisteredClassContext {
    n_ctx: u32,
    canonical_prefill_tokens: u32,
    canonical_decode_tokens: u32,
    max_context_tokens: u32,
}

/// **One `getPalwClassContexts` row**: the registered declaration first — with the ledger's model id
/// when the ledger knows the class — the build ledger second, and zeros named `unknown` last, so a
/// reader can tell a context the chain fixed from one this build supplied and from none at all.
///
/// The canonical footprint is computed here, from the row's own prefill and decode and through the
/// consensus spelling of the rule, for both sources — so a ledger cannot state one the chain would
/// compute differently.
fn palw_class_context_row(
    class_id: kaspa_hashes::Hash64,
    declared: Option<PalwRegisteredClassContext>,
    ledger: Option<PalwClassLedgerContext>,
) -> RpcPalwClassContext {
    let footprint = |prefill: u32, decode: u32| {
        u32::try_from(kaspa_consensus_core::palw_context_ladder::palw_job_footprint_v1(prefill, decode)).unwrap_or(u32::MAX)
    };
    match (declared, ledger) {
        (Some(declared), ledger) => RpcPalwClassContext {
            class_id: class_id.to_string(),
            model_id: ledger.map(|ledger| ledger.model_id).unwrap_or_default(),
            n_ctx: declared.n_ctx,
            canonical_prefill_tokens: declared.canonical_prefill_tokens,
            canonical_decode_tokens: declared.canonical_decode_tokens,
            canonical_footprint_positions: footprint(declared.canonical_prefill_tokens, declared.canonical_decode_tokens),
            max_context_tokens: declared.max_context_tokens,
            source: "chain_registration".to_string(),
        },
        (None, Some(ledger)) => RpcPalwClassContext {
            class_id: class_id.to_string(),
            model_id: ledger.model_id,
            n_ctx: ledger.n_ctx,
            canonical_prefill_tokens: ledger.canonical_prefill_tokens,
            canonical_decode_tokens: ledger.canonical_decode_tokens,
            canonical_footprint_positions: footprint(ledger.canonical_prefill_tokens, ledger.canonical_decode_tokens),
            max_context_tokens: ledger.max_context_tokens,
            source: "build_ledger".to_string(),
        },
        (None, None) => RpcPalwClassContext { class_id: class_id.to_string(), source: "unknown".to_string(), ..Default::default() },
    }
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

/// kaspa-pq: the lowercase wire string for a stake-bond status, shared by the stake-bond RPCs.
fn bond_status_str(status: kaspa_consensus_core::dns_finality::BondStatus) -> &'static str {
    use kaspa_consensus_core::dns_finality::BondStatus;
    match status {
        BondStatus::Pending => "pending",
        BondStatus::Active => "active",
        BondStatus::Unbonding => "unbonding",
        BondStatus::Slashed => "slashed",
    }
}

/// kaspa-pq: parse a `GetStakeBonds` status filter token (case-insensitive). A malformed value is a client error.
fn parse_bond_status(s: &str) -> RpcResult<kaspa_consensus_core::dns_finality::BondStatus> {
    use kaspa_consensus_core::dns_finality::BondStatus;
    match s.trim().to_ascii_lowercase().as_str() {
        "pending" => Ok(BondStatus::Pending),
        "active" => Ok(BondStatus::Active),
        "unbonding" => Ok(BondStatus::Unbonding),
        "slashed" => Ok(BondStatus::Slashed),
        other => Err(RpcError::General(format!("unknown stake-bond status '{other}' (expected pending/active/unbonding/slashed)"))),
    }
}

/// **A PALW claim's phase, named for a reader** — `(phase, void reason, the DAA it was entered
/// at)`, shared by the two ADR-0078 read calls so they cannot disagree about one claim.
///
/// The void reason is spelled out rather than folded into the phase because ADR-0078 Decision 4
/// says a derivation of a claim that later voids "is a derivation of a voided claim, and says so
/// when read" — and a consumer deciding whether to trust a provenance wants to know whether the
/// claim died of a timeout or of a court's fraud finding. Empty for every non-voided phase.
fn palw_claim_phase_named(phase: &kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2) -> (String, String, u64) {
    use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2 as P, PalwVoidReasonV2 as R};
    match phase {
        P::Provisional => ("provisional".to_string(), String::new(), 0),
        P::PanelBound { bound_daa } => ("panel_bound".to_string(), String::new(), *bound_daa),
        P::ReceiptLicensed { licensed_daa } => ("receipt_licensed".to_string(), String::new(), *licensed_daa),
        P::Final { final_daa } => ("final".to_string(), String::new(), *final_daa),
        // **ADR-0062 SA-1's phase, and it is NOT a void.** A claim under an open data-availability
        // accusation has stopped advancing but has lost nothing: the producer may still disclose
        // the named event and resume, and `resumed` says whether it already has. So it reports its
        // own name with an EMPTY void reason, beside the phases that are alive, rather than being
        // folded into `voided` — a consumer reading a provenance must be able to tell "this claim
        // is being asked a question" from "this claim died", and ADR-0078 Decision 4's whole point
        // is that the read says which. The DAA is the accusation's, because that is the one the
        // disclose deadline is derived from.
        P::DefaultDisputed { accused_daa, .. } => ("default_disputed".to_string(), String::new(), *accused_daa),
        P::Voided { voided_daa, reason } => {
            let reason = match reason {
                R::BindTimeout => "bind_timeout",
                R::ReceiptTimeout => "receipt_timeout",
                R::CourtFraud => "court_fraud",
                R::ProducerWithholding => "producer_withholding",
                R::NoCapablePanel => "no_capable_panel",
            };
            ("voided".to_string(), reason.to_string(), *voided_daa)
        }
    }
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
    /// `getPalwClassContexts`: the build's class ledger, where the node has one.
    palw_class_ledger_provider: Option<Arc<dyn PalwClassLedgerProvider>>,
}

const RPC_CORE: &str = "rpc-core";

impl RpcCoreService {
    /// The single definition of "this node is synced", behind `is_synced` on getInfo,
    /// getServerInfo and getSyncStatus.
    ///
    /// These three used to disagree — getSyncStatus checked for a transitional IBD state and the
    /// other two did not — which matters because the caller that acts on the answer is a validator
    /// deciding whether to sign. Three definitions meant the strictest caller could be told the
    /// most optimistic answer.
    ///
    /// `is_sink_recent_and_connected` already consults the chain-participation gate, so a node in
    /// IBD, candidate review, or quarantine reports unsynced here regardless of how recent its sink
    /// looks.
    async fn is_node_synced(&self, session: &ConsensusSessionOwned, sink_daa_score_timestamp: DaaScoreTimestamp) -> bool {
        self.mining_rule_engine.is_sink_recent_and_connected(sink_daa_score_timestamp)
            && !session.async_is_consensus_in_transitional_ibd_state().await
    }

    async fn palw_model_preflight_report(
        &self,
        session: ConsensusSessionOwned,
        bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        tip_daa: u64,
    ) -> RpcResult<(
        kaspa_consensus_core::palw_model_registration_v1::PalwModelPreflightReportV1,
        Option<kaspa_consensus_core::palw_state_v2::PalwClassRowV2>,
    )> {
        let class_id = match object {
            kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id, .. } => *class_id,
            _ => return Err(RpcError::General("the object is not a ClassRegistered".into())),
        };
        let rows = session.clone().spawn_blocking(|c| c.palw_v2_class_table()).await;
        let class_row = rows.into_iter().find(|r| r.class_id == class_id);
        let already = class_row.is_some();
        let registered_root = class_row.as_ref().map(|r| r.artifact_root);
        let families = session.spawn_blocking(|c| c.palw_certified_families_v1()).await;
        let chain_certified: Vec<_> = families
            .into_iter()
            .filter(|(lane, _, _)| *lane == kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt)
            .map(|(_, _, record)| record.family)
            .collect();
        let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
        let report = kaspa_consensus_core::palw_model_registration_v1::palw_model_preflight_v1(
            &self.config.params,
            bundle,
            object,
            &certified,
            &chain_certified,
            tip_daa,
            already,
            registered_root,
        )
        .map_err(RpcError::General)?;
        Ok((report, class_row))
    }

    async fn fill_registration_from_tx(&self, registration: &mut RpcPalwModelRegistration, txid: &str) -> RpcResult<()> {
        let txid = parse_hash64(txid, "transaction id")?;
        registration.transaction_id = txid.to_string();
        registration.submitted = true;
        let in_mempool = self.mining_manager.clone().get_transaction(txid, TransactionQuery::All).await.is_some();
        if in_mempool {
            registration.accepted = true;
            registration.mempool_accepted = true;
        } else if !registration.included && !registration.folded {
            registration.reject_code = kaspa_consensus_core::palw_model_registration_v1::PalwModelRegistrationCodeV1::RegistrationNotIncluded
                .code()
                .to_string();
            if registration.processor_verdict.is_empty() {
                registration.processor_verdict = registration.reject_code.clone();
            }
        }
        Ok(())
    }

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
            palw_class_ledger_provider: None,
        }
    }

    /// Hand `getPalwClassContexts` the build's class ledger. A consuming setter, so a node built
    /// without calling it — every construction site today — answers from the chain alone.
    pub fn with_palw_class_ledger_provider(mut self, provider: Option<Arc<dyn PalwClassLedgerProvider>>) -> Self {
        self.palw_class_ledger_provider = provider;
        self
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
        let session = self.consensus_manager.consensus().unguarded_session();
        let sink_daa_score_timestamp = session.async_get_sink_daa_score_timestamp().await;
        let is_synced = self.is_node_synced(&session, sink_daa_score_timestamp).await;
        Ok(GetInfoResponse {
            p2p_id: self.flow_context.node_id.to_string(),
            mempool_size: self.mining_manager.transaction_count_sample(TransactionQuery::TransactionsOnly),
            server_version: version().to_string(),
            is_utxo_indexed: self.config.utxoindex,
            is_synced,
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

    async fn get_token_ledger_entry_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetTokenLedgerEntryRequest,
    ) -> RpcResult<GetTokenLedgerEntryResponse> {
        // The token overlay is removed and no node holds a token ledger, so every request gets the
        // "not configured" answer (`available: false`). The op stays for wire compatibility.
        Ok(GetTokenLedgerEntryResponse::default())
    }

    // ------------------------------------------------------------------------------------------
    // ADR-0078 Decision 5 — verification belongs to the consumer, and the chain makes it possible
    // ------------------------------------------------------------------------------------------

    async fn get_palw_derived_artifacts_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwDerivedArtifactsRequest,
    ) -> RpcResult<GetPalwDerivedArtifactsResponse> {
        // **The read Decision 5 was promising.** A `DerivedArtifactV1` is a statement the chain
        // stores and never checks the content of; what makes it more than an assertion is that
        // anyone holding the answer can recompute `output_root`, `dsl_hash` and `artifact_hash`
        // and compare them with the chain's copy (X6). Until this call, the `derived_artifacts`
        // table had no reader outside the transition that wrote it — the object was in the state
        // root and nowhere a person could reach, which makes "publicly demonstrable by anyone
        // holding the DSL" a sentence about ids nobody could fetch.
        //
        // What is NOT returned: `output_token_ids`. They are not on this chain in any form — the
        // claim commits `output_commitment_v2(job_context_hash, ids, family_rendered_hash)` and
        // nothing else — and ADR-0044 Decision 8's sentence about not publishing prompts applies
        // to answers word for word. The consumer holds the ids from the gateway's response and
        // this call hands back the chain's side of the comparison.
        //
        // A malformed claim id is an ERROR, not `found: false`: "this chain does not hold that
        // claim" and "you typed 129 characters" must not share a reply.
        let claim_id = request
            .claim_id
            .parse::<kaspa_hashes::Hash64>()
            .map_err(|_| RpcError::General(format!("claim id '{}' is not a 128-hex Hash64", request.claim_id)))?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some((claim, executor_pubkey, rows)) = session.palw_derived_artifacts_v1(claim_id) else {
            return Ok(GetPalwDerivedArtifactsResponse { claim_id: claim_id.to_string(), ..Default::default() });
        };
        let (claim_phase, claim_void_reason, _phase_daa) = palw_claim_phase_named(&claim.phase);
        Ok(GetPalwDerivedArtifactsResponse {
            found: true,
            claim_id: claim_id.to_string(),
            output_root: claim.output_root.to_string(),
            executor_pubkey: faster_hex::hex_string(&executor_pubkey),
            executor_bond: format!("{}:{}", claim.bond.0.transaction_id, claim.bond.0.index),
            class_id: claim.class_id.to_string(),
            claim_phase,
            claim_void_reason,
            claim_accepted_block: claim.accepted_block.to_string(),
            claim_accepted_daa: claim.accepted_daa,
            artifacts: rows
                .into_iter()
                .map(|(key, row)| kaspa_rpc_core::RpcPalwDerivedArtifact {
                    transformer_id: key.transformer.to_string(),
                    derived_id: row.derived_id.to_string(),
                    grammar_id: row.grammar_id.to_string(),
                    kind: row.kind as u32,
                    // The chain interprets no kind (Decision 9 / X8); this is the shipped table's
                    // label for a human reader, and empty for an id this build has no name for.
                    kind_name: kaspa_consensus_core::palw_derived_v1::kind::name(row.kind).unwrap_or_default().to_string(),
                    dsl_hash: row.dsl_hash.to_string(),
                    artifact_hash: row.artifact_hash.to_string(),
                    artifact_bytes: row.artifact_bytes,
                    accepted_daa: row.accepted_daa,
                })
                .collect(),
        })
    }

    async fn get_palw_free_prompt_claim_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwFreePromptClaimRequest,
    ) -> RpcResult<GetPalwFreePromptClaimResponse> {
        // The claim beside the derivation (ADR-0077 R0). Same read, same session, so the two calls
        // cannot disagree about a claim: one function answers both.
        let claim_id = request
            .claim_id
            .parse::<kaspa_hashes::Hash64>()
            .map_err(|_| RpcError::General(format!("claim id '{}' is not a 128-hex Hash64", request.claim_id)))?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some((claim, executor_pubkey, rows)) = session.palw_derived_artifacts_v1(claim_id) else {
            return Ok(GetPalwFreePromptClaimResponse { claim_id: claim_id.to_string(), ..Default::default() });
        };
        let (phase, void_reason, phase_daa) = palw_claim_phase_named(&claim.phase);
        let (is_free_prompt, quanta, quanta_spent) = match &claim.source {
            kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta, spent } => {
                (true, *quanta, spent.len() as u32)
            }
            kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::Attempt => (false, 0, 0),
        };
        Ok(GetPalwFreePromptClaimResponse {
            found: true,
            claim_id: claim_id.to_string(),
            is_free_prompt,
            class_id: claim.class_id.to_string(),
            executor_pubkey: faster_hex::hex_string(&executor_pubkey),
            executor_bond: format!("{}:{}", claim.bond.0.transaction_id, claim.bond.0.index),
            output_root: claim.output_root.to_string(),
            trace_root: claim.trace_root.to_string(),
            execution_root: claim.execution_root.to_string(),
            work_leaves: claim.work_leaves,
            work_id: claim.work_id.map(|w| w.to_string()).unwrap_or_default(),
            quanta,
            quanta_spent,
            phase,
            void_reason,
            phase_daa,
            accepted_block: claim.accepted_block.to_string(),
            accepted_daa: claim.accepted_daa,
            trace_retention_daa: claim.trace_retention_daa,
            derived_count: rows.len() as u32,
        })
    }

    // ------------------------------------------------------------------------------------------
    // ADR-0080 design A — a declared court close, mid-assembly
    // ------------------------------------------------------------------------------------------

    async fn get_palw_pending_chunk_group_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwPendingChunkGroupRequest,
    ) -> RpcResult<GetPalwPendingChunkGroupResponse> {
        // **The read the split close was filing blind without.** A close too wide for one carrier
        // rides as a signed declaration and its chunks, one per block, under a court deadline. The
        // mover's only account of what had landed was `misaka palw court-close`'s journal on its
        // own disk — a file that believes itself, and therefore skips a part whose carrier was
        // reorged out and completes a group that can never assemble. It also could not answer the
        // two preflights that decide whether filing is worth anything: whether a declaration for
        // this `(session, side)` already exists (one per side, ever) and how much of the assembly
        // window is left.
        //
        // A malformed session id or an unknown side is an ERROR, not `found: false`: "this chain
        // holds no such group" and "you typed a side that does not exist" must not share a reply —
        // the first says keep filing, the second says you are asking about nothing.
        let session_id = request
            .session_id
            .parse::<kaspa_hashes::Hash64>()
            .map_err(|_| RpcError::General(format!("session id '{}' is not a 128-hex Hash64", request.session_id)))?;
        let side = kaspa_consensus_core::palw_state_v2::PalwCourtSideV1::from_name(request.side.trim()).ok_or_else(|| {
            RpcError::General(format!(
                "side '{}' is neither `challenger` nor `executor`, and a court session binds exactly those two bonds",
                request.side
            ))
        })?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(group) = session.palw_court_close_group_v1(session_id, side) else {
            return Ok(GetPalwPendingChunkGroupResponse {
                session_id: session_id.to_string(),
                side: side.name().to_string(),
                ..Default::default()
            });
        };
        Ok(GetPalwPendingChunkGroupResponse {
            found: true,
            session_id: session_id.to_string(),
            side: side.name().to_string(),
            count: group.count as u32,
            present: group.present,
            parts_present: group.present.count_ones(),
            complete: group.is_complete(),
            declared_daa: group.declared_daa,
            assembly_deadline_daa: group.assembly_deadline_daa,
            close_digest: group.close_digest.to_string(),
            verdict: match group.verdict {
                kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty => "executor_guilty",
                kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ChallengerDefeated => "challenger_defeated",
            }
            .to_string(),
            declarer_bond: format!("{}:{}", group.declarer.0.transaction_id, group.declarer.0.index),
            deposit: group.deposit,
        })
    }

    async fn get_palw_model_market_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelMarketRequest,
    ) -> RpcResult<GetPalwModelMarketResponse> {
        use kaspa_consensus_core::palw_model_market_v1::{PALW_MODEL_MARKET_VIRTUAL_SOMPI_V1, PALW_MODEL_SUPPLY_UNITS_V1};
        let line_id = parse_hash64(&request.line_id, "line id")?;
        let session = self.consensus_manager.consensus().unguarded_session();
        // ADR-0114: the schedule the fold would settle a move under at the virtual's DAA — served
        // with the row so a quote made off it is the fold's, and with the height it changes at.
        let params = &self.config.params;
        let schedule = params.palw_model_fees_at(session.get_virtual_daa_score());
        let leg_v2_activation_daa = params.palw_model_leg_v2_fence().map(|f| f.daa_score()).unwrap_or(0);
        // ADR-0120: the least seed the fold would open this pair at, at the virtual's DAA — served with
        // an unseeded line too, which is the one a seeder is reading it for.
        let seed_min_sompi = params.palw_model_seed_min_sompi_at(session.get_virtual_daa_score());
        let Some((market, opened, status)) = session.palw_model_market_v1(line_id) else {
            return Ok(GetPalwModelMarketResponse {
                line_id: line_id.to_string(),
                seed_min_sompi,
                burn_permille: schedule.burn_permille,
                leg_permille: schedule.leg_permille,
                leg_v2_activation_daa,
                ..Default::default()
            });
        };
        // **The 2026-09-23 Position route matrix, P-B3: the fold's own market gate**, asked at the
        // virtual's next block. A refusal closes the quote too, so a client that reads only
        // `closed_to_buys` — an older CLI, the options site — is not offered a buy the fold refuses.
        let gate = session.palw_model_market_gate_v1(line_id).unwrap_or_default();
        let market_refusal = gate.refusal.unwrap_or_default();
        Ok(GetPalwModelMarketResponse {
            found: true,
            line_id: line_id.to_string(),
            opened,
            opened_daa: market.opened_daa,
            msk_reserve: market.msk_reserve,
            position_units: market.position_units,
            sold_units: market.sold_units,
            burned_sompi: market.burned_sompi,
            registrant_paid_sompi: market.registrant_paid_sompi,
            closed_to_buys: market.closed_to_buys
                || !matches!(status, kaspa_consensus_core::palw_state_v2::PalwClassStatusV2::Active)
                || !market_refusal.is_empty(),
            price_sompi_per_position: market.price_sompi_per_position_v1(),
            supply_units: PALW_MODEL_SUPPLY_UNITS_V1,
            virtual_sompi: PALW_MODEL_MARKET_VIRTUAL_SOMPI_V1,
            class_status: format!("{status:?}"),
            contributor_paid_sompi: market.contributor_paid_sompi,
            seed_sompi: market.seed_sompi,
            seeded_by: if opened { market.seeded_by.to_string() } else { String::new() },
            seed_min_sompi,
            seed_pledged_sompi: market.seed_pledged_sompi,
            buyback_sompi: market.buyback_sompi,
            retired_units: market.retired_units,
            burn_permille: schedule.burn_permille,
            leg_permille: schedule.leg_permille,
            leg_v2_activation_daa,
            class_lifecycle: gate.lifecycle,
            market_refusal,
        })
    }

    async fn get_palw_model_line_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelLineRequest,
    ) -> RpcResult<GetPalwModelLineResponse> {
        let line_id = parse_hash64(&request.line_id, "line id")?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(read) = session.palw_model_line_v1(line_id) else {
            return Ok(GetPalwModelLineResponse { line_id: line_id.to_string(), ..Default::default() });
        };
        Ok(GetPalwModelLineResponse {
            exists: true,
            line_id: line_id.to_string(),
            line: Some(rpc_palw_model_line(&read.row)),
            current_root: read.current_root.map(|h| h.to_string()),
            roots_in_force: read.roots_in_force.iter().map(|h| h.to_string()).collect(),
            tip_daa: read.tip_daa,
            benefits: read.benefits.as_ref().map(rpc_palw_model_benefits),
            // ADR-0101: the facts, not a verdict. The node states what the chain says about the
            // line; `palw_service_descriptor_check_v1` runs in the client that holds the
            // descriptor, because a descriptor is a statement to a client and no rule reads one.
            service_facts: RpcPalwLineServiceFacts {
                declared_grants: read.service_facts.declared_grants,
                roots: read.service_facts.roots.iter().map(|h| h.to_string()).collect(),
                origin_pubkeys: read.service_facts.origin_pubkeys.iter().map(|k| faster_hex::hex_string(k)).collect(),
            },
        })
    }

    async fn get_palw_model_version_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelVersionRequest,
    ) -> RpcResult<GetPalwModelVersionResponse> {
        let line_id = parse_hash64(&request.line_id, "line id")?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(read) = session.palw_model_version_v1(line_id, request.version) else {
            return Ok(GetPalwModelVersionResponse {
                line_id: line_id.to_string(),
                version_number: request.version,
                ..Default::default()
            });
        };
        Ok(GetPalwModelVersionResponse {
            exists: true,
            line_id: line_id.to_string(),
            version_number: request.version,
            version: Some(rpc_palw_model_version(line_id, request.version, &read.version, read.tip_daa)),
            evaluations: read
                .evaluations
                .iter()
                .map(|(by, e)| RpcPalwModelEvaluation {
                    evaluator_id: e.evaluator_id.to_string(),
                    score_permille: e.score_permille,
                    report_hash: e.report_hash.to_string(),
                    posted_daa: e.posted_daa,
                    by: by.0.into(),
                    is_lines_own: e.is_lines_own,
                })
                .collect(),
            tip_daa: read.tip_daa,
        })
    }

    async fn get_palw_model_lines_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelLinesRequest,
    ) -> RpcResult<GetPalwModelLinesResponse> {
        let class_id = parse_hash64(&request.class_id, "class id")?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(rows) = session.palw_model_lines_v1(class_id) else {
            return Ok(GetPalwModelLinesResponse { class_id: class_id.to_string(), ..Default::default() });
        };
        Ok(GetPalwModelLinesResponse {
            exists: true,
            class_id: class_id.to_string(),
            lines: rows.iter().map(rpc_palw_model_line).collect(),
        })
    }

    async fn get_palw_model_proposals_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelProposalsRequest,
    ) -> RpcResult<GetPalwModelProposalsResponse> {
        let line_id = parse_hash64(&request.line_id, "line id")?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(proposals) = session.palw_model_proposals_v1(line_id) else {
            return Ok(GetPalwModelProposalsResponse { line_id: line_id.to_string(), ..Default::default() });
        };
        Ok(GetPalwModelProposalsResponse {
            exists: true,
            line_id: line_id.to_string(),
            proposals: proposals
                .iter()
                .map(|(id, p)| RpcPalwModelProposal {
                    proposal_id: id.to_string(),
                    line_id: p.line_id.to_string(),
                    root: p.root.to_string(),
                    note_hash: p.note_hash.to_string(),
                    by: p.by.0.into(),
                    posted_daa: p.posted_daa,
                    adopted_in: p.adopted_in,
                })
                .collect(),
        })
    }

    async fn get_palw_model_positions_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelPositionsRequest,
    ) -> RpcResult<GetPalwModelPositionsResponse> {
        let holder = parse_hash64(&request.holder, "holder")?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let positions = session
            .palw_model_positions_v1(holder)
            .into_iter()
            .map(|(line_id, units)| RpcPalwModelPosition { line_id: line_id.to_string(), units })
            .collect();
        Ok(GetPalwModelPositionsResponse { holder: holder.to_string(), positions })
    }

    // ------------------------------------------------------------------------------------------
    // ADR-0122 §6.5 — the operator's reads
    // ------------------------------------------------------------------------------------------

    async fn get_palw_claims_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwClaimsRequest,
    ) -> RpcResult<GetPalwClaimsResponse> {
        // Everything the caller sent is parsed before a byte of chain state is read (mainnet audit
        // M-5): a malformed request must be free, and it is an ERROR, not an empty list — "this bond
        // has no claims" and "you named no bond" must not share a reply.
        let (txid, index) =
            request.bond.split_once(':').ok_or_else(|| RpcError::General(format!("bond '{}' is not <txid>:<index>", request.bond)))?;
        let transaction_id = txid
            .parse::<kaspa_consensus_core::tx::TransactionId>()
            .map_err(|_| RpcError::General(format!("bond transaction id '{txid}' is not a transaction id")))?;
        let index: u32 = index.parse().map_err(|_| RpcError::General(format!("bond index '{index}' is not an output index")))?;
        let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
            transaction_id,
            index,
        });
        let role = match request.role.trim() {
            "" | "executor" => kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Executor,
            "seat" => kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Seat,
            other => return Err(RpcError::General(format!("role '{other}' is neither `executor` nor `seat`"))),
        };
        // A bound on what one call may cost the node: the state's claims are retired after
        // `claim_retirement`, so a bond's list is short in practice; the cap keeps it so.
        const CAP: usize = 500;
        let limit = match request.limit as usize {
            0 => CAP,
            n => n.min(CAP),
        };
        let include_terminal = request.include_terminal;
        let session = self.consensus_manager.consensus().unguarded_session();
        let read = session.spawn_blocking(move |c| c.palw_claim_rows_v1(bond, role, include_terminal, limit)).await;
        let role_name = match role {
            kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Executor => "executor",
            kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Seat => "seat",
        };
        let Some(read) = read else {
            return Ok(GetPalwClaimsResponse { bond: request.bond, role: role_name.to_string(), ..Default::default() });
        };
        let outpoint = |b: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2| format!("{}:{}", b.0.transaction_id, b.0.index);
        let claims = read
            .rows
            .iter()
            .map(|row| {
                let (phase, void_reason, phase_daa) = palw_claim_phase_named(&row.phase);
                RpcPalwClaimRow {
                    claim_id: row.claim_id.to_string(),
                    is_free_prompt: row.free_prompt,
                    class_id: row.class_id.to_string(),
                    executor_bond: outpoint(&row.executor_bond),
                    phase,
                    void_reason,
                    phase_daa,
                    accepted_daa: row.accepted_daa,
                    accepted_block: row.accepted_block.to_string(),
                    rebound_daa: row.rebound_daa,
                    bound_daa: row.bound_daa,
                    seats: row.seats.iter().map(outpoint).collect(),
                    deadline_daa: row.deadline_daa,
                    reserved_sompi: row.reserved.to_string(),
                    escrow_sompi: row.escrowed_reward,
                    payout_pending_sompi: row.payout_pending,
                    quanta: row.quanta,
                    quanta_spent: row.quanta_spent,
                    work_leaves: row.work_leaves,
                    open_courts: row.open_courts as u32,
                    exec_stage: row.exec_lane.as_ref().map(|lane| lane.stage.to_string()).unwrap_or_default(),
                    exec_credit: row.exec_lane.as_ref().map(|lane| lane.credit).unwrap_or(0),
                    exec_span: row.exec_lane.as_ref().map(|lane| lane.span),
                    exec_tickets: row.exec_lane.as_ref().map(|lane| lane.tickets).unwrap_or(0),
                    exec_tickets_spent: row.exec_lane.as_ref().map(|lane| lane.tickets_spent).unwrap_or(0),
                    exec_first_round: row.exec_lane.as_ref().and_then(|lane| lane.first_round),
                    exec_last_round: row.exec_lane.as_ref().and_then(|lane| lane.last_round),
                }
            })
            .collect();
        let summary = read.bond.as_ref();
        Ok(GetPalwClaimsResponse {
            available: true,
            tip_daa: read.tip_daa,
            bond: request.bond,
            role: role_name.to_string(),
            claims,
            truncated: read.truncated,
            bond_known: summary.is_some(),
            bond_pubkey: summary.map(|b| faster_hex::hex_string(&b.pubkey)).unwrap_or_default(),
            bond_retiring_since_daa: summary.and_then(|b| b.retiring_since_daa),
            bond_collateral: summary.map_or(0, |b| b.collateral),
            bond_slashed: summary.map_or(0, |b| b.slashed),
            bond_registered_daa: summary.map_or(0, |b| b.registered_daa),
            bond_capable_classes: summary.map(|b| b.capable_classes.iter().map(|c| c.to_string()).collect()).unwrap_or_default(),
        })
    }

    async fn get_palw_classes_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetPalwClassesRequest,
    ) -> RpcResult<GetPalwClassesResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let tip_daa = session.get_virtual_daa_score();
        let rows = session.spawn_blocking(|c| c.palw_v2_class_table()).await;
        Ok(GetPalwClassesResponse {
            available: !rows.is_empty(),
            tip_daa,
            classes: rows
                .into_iter()
                .map(|row| RpcPalwClassRow {
                    class_id: row.class_id.to_string(),
                    is_base_class: row.is_base_class,
                    status: row.status,
                    share_permille: row.share_permille,
                    budget_blocks: row.budget_blocks,
                    canonical_leaves: row.canonical_leaves,
                    artifact_root: row.artifact_root.to_string(),
                    fp_certified: row.fp_certified,
                    held: row.held,
                    registered_daa: row.registered_daa,
                })
                .collect(),
        })
    }

    async fn get_palw_node_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetPalwNodeStatusRequest,
    ) -> RpcResult<GetPalwNodeStatusResponse> {
        // The boot identity is read off the config this node runs — the same two values its
        // `Consensus params fingerprint:` / `Consensus fence schedule:` lines print — so "is this
        // node the release?" is a read, not a grep of a log that may have rolled (ADR-0122 §6.1).
        let params = &self.config.params;
        let rt = self.flow_context.palw_runtime();
        Ok(GetPalwNodeStatusResponse {
            consensus_params_id: params.consensus_params_id().to_string(),
            fence_schedule: params.fence_schedule_v1(),
            consensus_schedule_id: params.consensus_schedule_id().to_string(),
            producer_state: if rt.producer_state.is_empty() { "off".to_string() } else { rt.producer_state.to_string() },
            producer_reason: rt.producer_reason,
            producer_since_unix: rt.producer_since_unix,
            producer_bond: rt.producer_bond,
            producer_class: rt.producer_class,
            draws: rt.draws,
            produced_blocks: rt.produced_blocks,
            receipt_blocks: rt.receipt_blocks,
            network_lost: rt.network_lost,
            last_block: rt.last_block,
            last_block_unix: rt.last_block_unix,
            last_draw_unix: rt.last_draw_unix,
            panel_running: rt.panel_running,
            panel_submitter: rt.panel_submitter,
            retention_dir: self.flow_context.palw_retention_dir().map(|d| d.display().to_string()).unwrap_or_default(),
            memory_share_bytes: rt.memory_share_bytes,
            memory_headroom_bytes: rt.memory_headroom_bytes,
            memory_reserved_bytes: rt.memory_reserved_bytes,
            memory_available_bytes: rt.memory_available_bytes,
            memory_bounded: rt.memory_bounded,
            memory_holders: rt.memory_holders,
            lane_window_blocks: rt.lane_window_blocks,
            lane_work_blocks: rt.lane_work_blocks,
            lane_heartbeat_blocks: rt.lane_heartbeat_blocks,
            lane_last_work_daa: rt.lane_last_work_daa,
            lane_mix: rt.lane_mix,
            lane_alarm: rt.lane_alarm,
        })
    }

    async fn get_palw_registration_terms_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetPalwRegistrationTermsRequest,
    ) -> RpcResult<GetPalwRegistrationTermsResponse> {
        let base_class_id = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.base_class_id.to_string(),
            _ => return Ok(GetPalwRegistrationTermsResponse::default()),
        };
        let session = self.consensus_manager.consensus().unguarded_session();
        let tip_daa = session.get_virtual_daa_score();
        let (terms, families) = session.spawn_blocking(|c| (c.palw_v2_registration_terms(), c.palw_certified_families_v1())).await;
        let Some(terms) = terms else {
            return Ok(GetPalwRegistrationTermsResponse { base_class_id, tip_daa, ..Default::default() });
        };
        let lane_name = |lane: kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1| match lane {
            kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt => "attempt",
            kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt => "free_prompt",
        };
        Ok(GetPalwRegistrationTermsResponse {
            available: true,
            tip_daa,
            base_class_id,
            min_grantable_share_permille: terms.min_grantable_share_permille,
            slash_value_per_pwu: terms.slash_value_per_pwu,
            initial_target: terms.initial_target.to_string(),
            registered_class_ids: terms.registered_class_ids.iter().map(|c| c.to_string()).collect(),
            registered_artifact_roots: terms.registered_artifact_roots.iter().map(|r| r.to_string()).collect(),
            families: families
                .into_iter()
                .map(|(lane, digest, record)| RpcPalwCertifiedFamily {
                    lane: lane_name(lane).to_string(),
                    digest: digest.to_string(),
                    certified_daa: record.certified_daa,
                    family_hex: faster_hex::hex_string(&borsh::to_vec(&record.family).unwrap_or_default()),
                })
                .collect(),
        })
    }

    async fn get_palw_round_lane_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetPalwRoundLaneRequest,
    ) -> RpcResult<GetPalwRoundLaneResponse> {
        let Some(lane) = self.config.params.palw_execution_lane_fence() else {
            return Ok(GetPalwRoundLaneResponse::default());
        };
        let stages = std::iter::once(RpcPalwRoundLaneStage {
            activation_daa: lane.activation.daa_score(),
            permits_per_round: lane.permits_per_round,
        })
        .chain(lane.widenings.iter().filter(|stage| stage.is_used()).map(|stage| RpcPalwRoundLaneStage {
            activation_daa: stage.activation.daa_score(),
            permits_per_round: stage.permits_per_round,
        }))
        .collect();
        let session = self.consensus_manager.consensus().unguarded_session();
        let virtual_daa = session.get_virtual_daa_score();
        let round = kaspa_consensus_core::palw_execution_lane_v1::palw_execution_round_v1(
            kaspa_core::time::unix_now(),
            self.config.params.genesis.timestamp,
        );
        let mut response = GetPalwRoundLaneResponse {
            armed: true,
            schedule_span_daa: lane.schedule_span_daa_at(virtual_daa),
            max_per_mergeset: lane.max_per_mergeset,
            stages,
            virtual_daa,
            round,
            ..Default::default()
        };
        let Some(status) = session.async_palw_round_lane_status_v1(round).await else {
            return Ok(response);
        };
        let bond_string =
            |bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2| format!("{}:{}", bond.0.transaction_id, bond.0.index);
        response.open = true;
        response.span = status.view.span;
        response.permits_per_round = status.view.width;
        response.permits = status
            .view
            .permits
            .iter()
            .map(|permit| RpcPalwRoundPermit {
                index: permit.index,
                bond: bond_string(&permit.bond),
                operator_id: permit.operator_id.to_string(),
                domain: permit.domain.to_string(),
                used: status.view.used.contains(&permit.index),
            })
            .collect();
        response.domains = status
            .schedule
            .iter()
            .flat_map(|schedule| schedule.domains.iter())
            .map(|domain| RpcPalwRoundLaneDomain {
                domain: domain.domain.to_string(),
                credits: domain.credits,
                quota_permille: domain.quota_permille,
                parity: domain.parity,
                bonds: domain.bonds.len() as u32,
            })
            .collect();
        response.accepted_in_span = status.accepted_in_span;
        response.finals_span = status.finals_span;
        response.finals = status.finals;
        response.next_round_permits = status
            .schedule
            .as_ref()
            .map(|schedule| {
                kaspa_consensus_core::palw_execution_lane_v1::palw_execution_permits_v2(
                    schedule,
                    status.view.round + 1,
                    status.view.width,
                    status.tickets_only,
                )
                .len() as u16
            })
            .unwrap_or(0);
        Ok(response)
    }

    async fn get_palw_settlement_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwSettlementRequest,
    ) -> RpcResult<GetPalwSettlementResponse> {
        // ADR-0127 Decision 3: one read of the sink's PALW state. Unavailable is not unsettled — a
        // node with no V2 state, or one that cannot date its frontier, says so.
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(settlement) = session.async_palw_settlement_v1(request.daa_score).await else {
            let sink_daa = session.async_get_sink_daa_score_timestamp().await.daa_score;
            return Ok(GetPalwSettlementResponse { sink_daa, daa_score: request.daa_score, ..Default::default() });
        };
        Ok(GetPalwSettlementResponse {
            available: true,
            sink_daa: settlement.sink_daa,
            daa_score: request.daa_score,
            settled: settlement.settled,
            depth: settlement.depth,
            pending_anchors: settlement.pending,
            depth_is_lower_bound: settlement.depth_is_lower_bound,
            safe_frontier_blue_score: settlement.safe_frontier_blue_score,
            safe_frontier_daa: settlement.safe_frontier_daa,
        })
    }

    async fn get_precommit_duty_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPrecommitDutyRequest,
    ) -> RpcResult<GetPrecommitDutyResponse> {
        // MISAKA §5 round 2: the duty view, read from the chain rather than remembered by the
        // signer. A malformed id or outpoint is a request error; `available: false` when the node
        // has no view for this validator.
        let validator_id = request
            .validator_id
            .parse::<kaspa_hashes::Hash64>()
            .map_err(|_| RpcError::General(format!("validator_id '{}' is not a valid 64-byte Hash64", request.validator_id)))?;
        let bond_outpoint = parse_bond_outpoint(&request.bond_outpoint)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        Ok(match session.async_get_precommit_duty(validator_id, bond_outpoint).await {
            Some(duty) => GetPrecommitDutyResponse {
                available: true,
                round_active: duty.round_active,
                sink_daa_score: duty.sink_daa_score,
                held_epoch: duty.held.epoch,
                held_anchor: duty.held.anchor.to_string(),
                due: duty
                    .due
                    .into_iter()
                    .map(|(epoch, anchor, anchor_daa_score, snapshot_commitment)| RpcPrecommitDue {
                        epoch,
                        anchor_hash: anchor.to_string(),
                        anchor_daa_score,
                        snapshot_commitment: snapshot_commitment.to_string(),
                    })
                    .collect(),
            },
            None => GetPrecommitDutyResponse::default(),
        })
    }

    async fn get_palw_class_contexts_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetPalwClassContextsRequest,
    ) -> RpcResult<GetPalwClassContextsResponse> {
        let (fp_max_prompt_tokens, fp_max_decode_tokens) = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                (bundle.freeprompt.max_prompt_tokens(), bundle.freeprompt.max_decode_tokens())
            }
            _ => (0, 0),
        };
        let session = self.consensus_manager.consensus().unguarded_session();
        let registered = session
            .spawn_blocking(|c| {
                c.palw_v2_class_table()
                    .into_iter()
                    .map(|row| {
                        let declared =
                            c.palw_registered_class_carriage_v1(row.class_id).map(|(profile, canonical)| PalwRegisteredClassContext {
                                n_ctx: profile.n_ctx,
                                canonical_prefill_tokens: canonical.declared_prefill_tokens,
                                canonical_decode_tokens: canonical.exact_decode_tokens,
                                max_context_tokens: canonical.max_context_tokens,
                            });
                        (row.class_id, declared)
                    })
                    .collect::<Vec<_>>()
            })
            .await;
        let ledger = self.palw_class_ledger_provider.as_deref();
        let classes: Vec<RpcPalwClassContext> = registered
            .into_iter()
            .map(|(class_id, declared)| palw_class_context_row(class_id, declared, ledger.and_then(|l| l.class_context(class_id))))
            .collect();
        Ok(GetPalwClassContextsResponse { available: !classes.is_empty(), fp_max_prompt_tokens, fp_max_decode_tokens, classes })
    }

    async fn get_palw_model_registry_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetPalwModelRegistryRequest,
    ) -> RpcResult<GetPalwModelRegistryResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(read) = session.spawn_blocking(|c| c.palw_model_registry_v1()).await else {
            return Ok(GetPalwModelRegistryResponse::default());
        };
        let globals = read.globals;
        let classes = read
            .classes
            .iter()
            .map(|class| {
                let row = class.row.as_ref();
                RpcPalwModelLifecycle {
                    class_id: class.class_id.to_string(),
                    artifact_root: class.artifact_root.to_string(),
                    is_base_class: class.is_base_class,
                    has_row: row.is_some(),
                    state: row.map(|r| format!("{:?}", r.state)).unwrap_or_else(|| "Legacy".to_string()),
                    since_span: row.map(|r| r.since_span).unwrap_or(0),
                    verification_ccu: row.map(|r| r.work.verification_ccu.to_string()).unwrap_or_else(|| "0".to_string()),
                    economic_ccu_per_claim: row.map(|r| r.work.economic_ccu_per_claim.to_string()).unwrap_or_else(|| "0".to_string()),
                    artifact_bytes: row.map(|r| r.work.artifact_bytes).unwrap_or(0),
                    ops_supported: row.map(|r| r.work.ops_supported).unwrap_or(false),
                    verification_window_spans: row.map(|r| r.profile.verification_window_spans).unwrap_or(0),
                    artifact_prefetch_spans: row.map(|r| r.profile.artifact_prefetch_spans).unwrap_or(0),
                    max_inflight_claims: row.map(|r| r.profile.max_inflight_claims).unwrap_or(0),
                    required_ready_seats: row.map(|r| r.profile.required_ready_seats).unwrap_or(0),
                    registration_bond_sompi: row.map(|r| r.profile.registration_bond_sompi).unwrap_or(0),
                    admission_claims_per_span_milli: row.map(|r| r.profile.admission_claims_per_span_milli).unwrap_or(0),
                    probes_passed: row.map(|r| r.probes_passed).unwrap_or(0),
                    probes_failed: row.map(|r| r.probes_failed).unwrap_or(0),
                    ready_seats: row.map(|r| r.ready_seats).unwrap_or(0),
                    inflight_claims: row.map(|r| r.inflight_claims).unwrap_or(0),
                    utilization_permille: row.map(|r| r.utilization_permille).unwrap_or(0),
                    admission_milli: row.map(|r| r.admission_milli).unwrap_or(0),
                    cap_utilization_permille: row.map(|r| r.cap_utilization_permille).unwrap_or(0),
                    priced_share_permille: row.map(|r| r.priced_share_permille).unwrap_or(0),
                    work_ratio_permille: class.work_ratio_permille,
                    expected_forwards_q32: class.expected_forwards_q32.to_string(),
                    work_ticket_target: class.work_ticket_target.to_string(),
                    class_target: class.class_target.to_string(),
                    panel_room: class.panel_room,
                    final_work_share_10_permille: class.final_work_share_10_permille,
                    final_work_share_100_permille: class.final_work_share_100_permille,
                    ready_seats_now: class.ready_seats_now,
                    inflight_now: class.inflight_now,
                    share_permille: class.share_permille.unwrap_or(0),
                    no_capable_panel_voids: class.no_capable_panel_voids,
                    reason: class.reason.clone(),
                }
            })
            .collect();
        let readiness = read
            .readiness
            .iter()
            .map(|r| RpcPalwSeatReadiness {
                bond_txid: r.bond.0.transaction_id.to_string(),
                bond_index: r.bond.0.index,
                class_id: r.class_id.to_string(),
                proved_daa: r.row.proved_daa,
                proved_span: r.row.proved_span,
                leaf_index: r.row.leaf_index,
                fresh: r.fresh,
                not_ready_reason: r.not_ready_reason.clone(),
            })
            .collect();
        Ok(GetPalwModelRegistryResponse {
            available: true,
            tip_daa: read.tip_daa,
            scheduled: read.fence_daa.is_some(),
            fence_daa: read.fence_daa.unwrap_or(0),
            active: read.active,
            grace_until_daa: read.grace_until_daa,
            span_daa: read.span_daa,
            reference_work_per_span: globals.map(|g| g.reference_work_per_span.to_string()).unwrap_or_else(|| "0".to_string()),
            reference_bytes_per_span: globals.map(|g| g.reference_bytes_per_span).unwrap_or(0),
            seat_count: globals.map(|g| g.seat_count).unwrap_or(0),
            spare_seats: globals.map(|g| g.spare_seats).unwrap_or(0),
            utilization_permille: globals.map(|g| g.utilization_permille).unwrap_or(0),
            probation_claims: globals.map(|g| g.probation_claims).unwrap_or(0),
            stable_epochs: globals.map(|g| g.stable_epochs).unwrap_or(0),
            readiness_probe_max_age_spans: globals.map(|g| g.readiness_probe_max_age_spans).unwrap_or(0),
            readiness_collateral_multiple: globals.map(|g| g.readiness_collateral_multiple).unwrap_or(0),
            classes,
            readiness,
            classes_active: read.counts[0],
            classes_active_limited: read.counts[1],
            classes_probation: read.counts[2],
            classes_prefetching: read.counts[3],
            classes_registered: read.counts[4],
            classes_held: read.counts[5],
            bonds_active: read.bonds.iter().filter(|b| b.active).count() as u32,
            work_target_shadow: read.work_target.is_some(),
            work_target: read.work_target.map(|w| w.target.work.to_string()).unwrap_or_default(),
            work_floor: read.work_target.map(|w| w.target.floor.to_string()).unwrap_or_default(),
            work_network_draws_q32: read.work_target.map(|w| w.target.network_draws_q32.to_string()).unwrap_or_default(),
            work_effective: read.work_target.map(|w| w.target.effective_work.to_string()).unwrap_or_default(),
            work_epoch_index: read.work_target.map(|w| w.target.epoch_index).unwrap_or(0),
            work_closed_model_blocks: read.work_target.map(|w| w.target.closed_model_blocks).unwrap_or(0),
            work_closed_expected_blocks: read.work_target.map(|w| w.target.closed_expected_blocks).unwrap_or(0),
            work_rate_sompi_per_giga: read.work_target.map(|w| w.rate_sompi_per_giga).unwrap_or(0),
            panel_inflight_replay: read.work_target.map(|w| w.panel_inflight_replay.to_string()).unwrap_or_default(),
            panel_horizon_spans: read.work_target.map(|w| w.panel_horizon_spans).unwrap_or(0),
            final_work_epochs: read.work_target.map(|w| w.final_work_epochs).unwrap_or(0),
            bonds_with_headroom: read
                .bonds
                .iter()
                .filter(|b| b.active && b.above_floor && b.free_collateral_sompi >= b.needed_collateral_sompi)
                .count() as u32,
        })
    }

    async fn get_palw_class_economics_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetPalwClassEconomicsRequest,
    ) -> RpcResult<GetPalwClassEconomicsResponse> {
        use kaspa_consensus_core::palw_economic_compute_v1::{
            PALW_ECONOMIC_COMPUTE_VERSION_V1, PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1,
            palw_job_economic_compute_v1,
        };
        // ADR-0131 Decision 1: the census the chain holds, and each class's compute from the graph it
        // registered (the carriage) or, for a genesis class that carried none, from this build.
        let seat_count = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.panel.seat_count(),
            _ => 0,
        };
        let session = self.consensus_manager.consensus().unguarded_session();
        // ADR-0132: the ledger is brought up to the chain first, off the async runtime — it reads
        // consensus and writes the node's database.
        if let Some(provider) = self.palw_class_ledger_provider.clone() {
            let refresh_session = self.consensus_manager.consensus().unguarded_session();
            refresh_session.spawn_blocking(move |_| provider.refresh_economics_ledger()).await;
        }
        let read = session
            .spawn_blocking(|c| {
                c.palw_class_census_v1().map(|census| {
                    let carriages: Vec<_> =
                        census.classes.iter().map(|row| c.palw_registered_class_carriage_v1(row.class_id)).collect();
                    (census, carriages)
                })
            })
            .await;
        let Some((census, carriages)) = read else { return Ok(GetPalwClassEconomicsResponse::default()) };
        let prefill_draw = self.config.params.palw_prefill_draw_active_at(census.tip_daa);
        let ledger = self.palw_class_ledger_provider.as_deref();
        let classes: Vec<RpcPalwClassEconomics> = census
            .classes
            .into_iter()
            .zip(carriages)
            .map(|(row, carriage)| {
                let (job, canonical, source) = match carriage {
                    Some((profile, canonical_job)) => {
                        let table = &PALW_ECONOMIC_COST_TABLE_V1;
                        match (
                            palw_attempt_economic_compute_v1(&profile, &canonical_job, prefill_draw, table),
                            palw_job_economic_compute_v1(&profile, &canonical_job, table),
                        ) {
                            (Ok(job), Ok(canonical)) => (job, canonical, "chain_registration"),
                            _ => (0, 0, "unknown"),
                        }
                    }
                    None => match ledger.and_then(|l| l.class_economic_compute(row.class_id)) {
                        Some(compute) => {
                            (if prefill_draw { compute.draw } else { compute.canonical }, compute.canonical, "build_ledger")
                        }
                        None => (0, 0, "unknown"),
                    },
                };
                RpcPalwClassEconomics {
                    class_id: row.class_id.to_string(),
                    model_id: ledger.and_then(|l| l.class_context(row.class_id)).map(|c| c.model_id).unwrap_or_default(),
                    is_base_class: row.is_base_class,
                    status: row.status,
                    share_permille: row.share_permille.unwrap_or(0),
                    pwu_per_inference: row.pwu_per_inference,
                    class_target: row.class_target.to_string(),
                    expected_attempts: row.expected_attempts,
                    expected_attempts_q32: row.expected_attempts_q32.to_string(),
                    economic_compute_job: job.to_string(),
                    economic_compute_canonical: canonical.to_string(),
                    economic_source: source.to_string(),
                    claims_accepted: row.claims_accepted,
                    claims_provisional: row.claims_provisional,
                    claims_panel_bound: row.claims_panel_bound,
                    claims_licensed: row.claims_licensed,
                    claims_final: row.claims_final,
                    claims_voided: row.claims_voided,
                    claims_redrawn: row.claims_redrawn,
                    escrow_accepted_sompi: row.escrow_accepted_sompi.to_string(),
                    escrow_final_sompi: row.escrow_final_sompi.to_string(),
                    ledger: ledger
                        .and_then(|l| l.class_ledger_totals(row.class_id))
                        .map(|t| rpc_palw_class_ledger_totals(&t))
                        .unwrap_or_default(),
                    telemetry: ledger
                        .and_then(|l| l.class_node_telemetry(row.class_id))
                        .map(rpc_palw_class_node_telemetry)
                        .unwrap_or_default(),
                    eligible_seats: row.eligible_seats,
                    duty_seats_inflight: row.duty_seats_inflight,
                    seat_exposure_inflight_sompi: row.seat_exposure_inflight_sompi.to_string(),
                    free_collateral_sompi: row.free_collateral_sompi.to_string(),
                }
            })
            .collect();
        let summary = ledger.and_then(|l| l.economics_ledger_summary());
        Ok(GetPalwClassEconomicsResponse {
            available: !classes.is_empty(),
            tip_daa: census.tip_daa,
            economic_compute_version: PALW_ECONOMIC_COMPUTE_VERSION_V1,
            seat_count,
            prefill_draw,
            network_bits: census.network_bits,
            network_expected_attempts_q32: census.network_expected_attempts_q32.to_string(),
            ledger_available: summary.is_some(),
            ledger_claims: summary.map(|s| s.claims).unwrap_or(0),
            ledger_first_daa: summary.map(|s| s.first_daa).unwrap_or(0),
            ledger_last_daa: summary.map(|s| s.last_daa).unwrap_or(0),
            classes,
        })
    }

    async fn get_palw_free_prompt_price_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwFreePromptPriceRequest,
    ) -> RpcResult<GetPalwFreePromptPriceResponse> {
        // **ADR-0148: the entrance prices with the ledger's expression.** A gateway used to size a
        // commitment's exposure in step leaves after the ledger had moved to compute, and past the
        // bundle a wide model's claim reserved several times what its gateway had checked — a
        // commitment admitted at the entrance and refused at the transition, after the carrier fee.
        // This answers with the fold's own function over the tip state, at the DAA a commitment
        // sent now lands at.
        //
        // Everything the caller sent is parsed before a byte of chain state is read (mainnet audit
        // M-5): a malformed request is an error and it is free. The token list is bounded by the
        // widest context the structural ladder admits, so the request cannot be made to walk an
        // unbounded prefix comparison.
        let class_id = request
            .class_id
            .parse::<kaspa_hashes::Hash64>()
            .map_err(|_| RpcError::General(format!("class id '{}' is not a 128-hex Hash64", request.class_id)))?;
        if request.prompt_token_ids.len() > (1usize << 21) {
            return Err(RpcError::General(format!(
                "{} prompt ids is past any context a class can register",
                request.prompt_token_ids.len()
            )));
        }
        let bond = if request.bond.is_empty() {
            None
        } else {
            let (txid, index) =
                request.bond.split_once(':').ok_or_else(|| RpcError::General(format!("bond '{}' is not txid:index", request.bond)))?;
            let transaction_id = txid
                .parse::<kaspa_consensus_core::tx::TransactionId>()
                .map_err(|_| RpcError::General(format!("bond transaction id '{txid}' is not a 128-hex transaction id")))?;
            let index = index.parse::<u32>().map_err(|_| RpcError::General(format!("bond index '{index}' is not a u32")))?;
            Some(kaspa_consensus_core::tx::TransactionOutpoint { transaction_id, index })
        };
        let GetPalwFreePromptPriceRequest { prompt_token_ids, prompt_tokens, decode_tokens_executed, work_leaves, .. } = request;
        let session = self.consensus_manager.consensus().unguarded_session();
        let answer = session
            .spawn_blocking(move |c| {
                c.palw_fp_commitment_price_v1(class_id, prompt_token_ids, prompt_tokens, decode_tokens_executed, work_leaves, bond)
            })
            .await;
        let Some(answer) = answer else {
            return Ok(GetPalwFreePromptPriceResponse::default());
        };
        let bond_room_sompi = answer.bond_room.map(|room| room.to_string()).unwrap_or_default();
        Ok(match answer.price {
            Ok(price) => GetPalwFreePromptPriceResponse {
                available: true,
                daa_score: answer.daa_score,
                priced: true,
                refusal: String::new(),
                priced_in_compute: price.priced_in_compute,
                quanta: price.quanta,
                pwu: price.pwu,
                // The whole reservation the ledger will hold — the weight and, past the audit fence, a
                // compute-priced claim's receipt rights (#5) — because a gateway checks exactly this
                // against `bond_room_sompi`.
                reserved_sompi: price.reserved.saturating_add(price.rights_reserved).to_string(),
                bond_room_sompi,
            },
            Err(refusal) => GetPalwFreePromptPriceResponse {
                available: true,
                daa_score: answer.daa_score,
                priced: false,
                refusal: format!("{refusal:?}"),
                bond_room_sompi,
                ..Default::default()
            },
        })
    }

    async fn get_palw_class_panel_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwClassPanelStatusRequest,
    ) -> RpcResult<GetPalwClassPanelStatusResponse> {
        let class_id = parse_class_id_or_alias(&request.class_id)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(view) = session.spawn_blocking(|c| c.palw_panel_network_view_v1()).await else {
            return Ok(GetPalwClassPanelStatusResponse::default());
        };
        let Some(row) = view.classes.into_iter().find(|c| c.class_id == class_id) else {
            return Ok(GetPalwClassPanelStatusResponse { available: true, tip_daa: view.tip_daa, found: false, status: Default::default() });
        };
        let holds_local = self
            .flow_context
            .palw_runtime()
            .panel_classes
            .iter()
            .filter(|c| c.class_id.eq_ignore_ascii_case(&class_id.to_string()))
            .count() as u32;
        Ok(GetPalwClassPanelStatusResponse {
            available: true,
            tip_daa: view.tip_daa,
            found: true,
            status: rpc_palw_class_panel_status(&row, holds_local),
        })
    }

    async fn get_palw_panel_seats_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwPanelSeatsRequest,
    ) -> RpcResult<GetPalwPanelSeatsResponse> {
        let class_filter = if request.class_id.trim().is_empty() {
            None
        } else {
            Some(parse_class_id_or_alias(&request.class_id)?)
        };
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(view) = session.spawn_blocking(|c| c.palw_panel_network_view_v1()).await else {
            return Ok(GetPalwPanelSeatsResponse::default());
        };
        let seats = view
            .seats
            .into_iter()
            .filter(|s| class_filter.is_none_or(|id| s.class_id == id))
            .map(rpc_palw_panel_seat)
            .collect();
        Ok(GetPalwPanelSeatsResponse { available: true, tip_daa: view.tip_daa, seats })
    }

    async fn get_palw_panel_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwPanelStatusRequest,
    ) -> RpcResult<GetPalwPanelStatusResponse> {
        let class_filter = if request.class_id.trim().is_empty() {
            None
        } else {
            Some(parse_class_id_or_alias(&request.class_id)?)
        };
        let session = self.consensus_manager.consensus().unguarded_session();
        let sink_daa_score_timestamp = session.async_get_sink_daa_score_timestamp().await;
        let synced = self.is_node_synced(&session, sink_daa_score_timestamp).await;
        let view = session.spawn_blocking(|c| c.palw_panel_network_view_v1()).await;
        let rt = self.flow_context.palw_runtime();
        let Some(view) = view else {
            return Ok(GetPalwPanelStatusResponse {
                panel_running: rt.panel_running,
                panel_submitter: rt.panel_submitter,
                synced,
                ..Default::default()
            });
        };
        let mut classes = Vec::new();
        for local in rt.panel_classes.iter().filter(|c| {
            class_filter.is_none_or(|id| c.class_id.eq_ignore_ascii_case(&id.to_string()))
        }) {
            let class_id = local.class_id.parse::<kaspa_hashes::Hash64>().ok();
            let chain = class_id.and_then(|id| view.classes.iter().find(|c| c.class_id == id));
            let seat = class_id.and_then(|id| {
                view.seats.iter().find(|s| {
                    s.class_id == id && kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(s.seat_id) == local.seat_id
                })
            });
            classes.push(RpcPalwLocalPanelClass {
                class_id: local.class_id.clone(),
                model_name: if local.model_name.is_empty() {
                    chain.map(|c| c.model_name.clone()).unwrap_or_default()
                } else {
                    local.model_name.clone()
                },
                seat_id: local.seat_id.clone(),
                artifact_loaded: local.artifact_loaded,
                artifact_root: local.artifact_root.clone(),
                working_set_bytes: local.working_set_bytes,
                replay_capable: local.replay_capable,
                synced,
                bond_active: seat.map(|s| s.eligible || s.ready).unwrap_or(false),
                collateral_sompi: seat.map(|s| s.collateral_available.min(u128::from(u64::MAX)) as u64).unwrap_or(0),
                readiness_proof_accepted: seat.map(|s| s.ready).unwrap_or(false),
                readiness_proved_daa: seat.map(|s| s.readiness_proved_daa).unwrap_or(0),
                chain_state: chain.map(|c| c.registry_state.clone()).unwrap_or_default(),
                assignments: seat.map(|s| s.assigned).unwrap_or(0),
                hold: local_hold_or_chain(local, seat),
                runtime_profile: local.runtime_profile.clone(),
                artifact_resident_bytes: local.artifact_resident_bytes,
                producer_working_set_bytes: local.producer_working_set_bytes,
                full_seat_working_set_bytes: local.full_seat_working_set_bytes,
                partial_seat_working_set_bytes: local.partial_seat_working_set_bytes,
                producer_capable: local.producer_capable,
                full_seat_capable: local.full_seat_capable,
                partial_seat_capable: local.partial_seat_capable,
            });
        }
        if classes.is_empty() {
            for row in view.classes.iter().filter(|c| class_filter.is_none_or(|id| c.class_id == id)) {
                let seat = view.seats.iter().find(|s| s.class_id == row.class_id);
                classes.push(RpcPalwLocalPanelClass {
                    class_id: row.class_id.to_string(),
                    model_name: row.model_name.clone(),
                    seat_id: seat.map(|s| kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(s.seat_id)).unwrap_or_default(),
                    artifact_loaded: false,
                    artifact_root: String::new(),
                    working_set_bytes: 0,
                    replay_capable: false,
                    runtime_profile: String::new(),
                    artifact_resident_bytes: 0,
                    producer_working_set_bytes: 0,
                    full_seat_working_set_bytes: 0,
                    partial_seat_working_set_bytes: 0,
                    producer_capable: false,
                    full_seat_capable: false,
                    partial_seat_capable: false,
                    synced,
                    bond_active: seat.map(|s| s.eligible || s.ready).unwrap_or(false),
                    collateral_sompi: seat.map(|s| s.collateral_available.min(u128::from(u64::MAX)) as u64).unwrap_or(0),
                    readiness_proof_accepted: seat.map(|s| s.ready).unwrap_or(false),
                    readiness_proved_daa: seat.map(|s| s.readiness_proved_daa).unwrap_or(0),
                    chain_state: row.registry_state.clone(),
                    assignments: seat.map(|s| s.assigned).unwrap_or(0),
                    hold: if rt.panel_running {
                        Some(rpc_hold(kaspa_consensus_core::palw_panel_view_v1::PalwPanelHoldReasonV1::NoArtifact))
                    } else {
                        seat.and_then(|s| s.hold).map(rpc_hold)
                    },
                });
            }
        }
        Ok(GetPalwPanelStatusResponse {
            available: true,
            tip_daa: view.tip_daa,
            panel_running: rt.panel_running,
            panel_submitter: rt.panel_submitter,
            synced,
            classes,
        })
    }

    async fn get_palw_panel_assignments_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwPanelAssignmentsRequest,
    ) -> RpcResult<GetPalwPanelAssignmentsResponse> {
        let claim_filter = if request.claim_id.trim().is_empty() {
            None
        } else {
            Some(parse_hash64(&request.claim_id, "claim id")?)
        };
        let seat_filter = if request.seat_id.trim().is_empty() { None } else { Some(request.seat_id.trim().to_string()) };
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(view) = session.spawn_blocking(|c| c.palw_panel_network_view_v1()).await else {
            return Ok(GetPalwPanelAssignmentsResponse::default());
        };
        let truncated = view.assignments.len() >= 512;
        let assignments = view
            .assignments
            .into_iter()
            .filter(|a| claim_filter.is_none_or(|id| a.claim_id == id))
            .filter(|a| {
                seat_filter.as_ref().is_none_or(|want| {
                    a.seats.iter().any(|s| kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(s.seat_id) == *want)
                        || kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(a.full_seat) == *want
                })
            })
            .map(rpc_palw_panel_assignment)
            .collect();
        Ok(GetPalwPanelAssignmentsResponse { available: true, tip_daa: view.tip_daa, truncated, assignments })
    }

    async fn get_palw_model_preflight_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelPreflightRequest,
    ) -> RpcResult<GetPalwModelPreflightResponse> {
        let Some(bundle) = palw_v2_bundle(&self.config.params) else {
            return Ok(GetPalwModelPreflightResponse::default());
        };
        let session = self.consensus_manager.consensus().unguarded_session();
        let tip_daa = session.get_virtual_daa_score();
        let object = decode_class_registered_hex(&request.object_hex)?;
        let (report, _) = self.palw_model_preflight_report(session, bundle, &object, tip_daa).await?;
        Ok(rpc_preflight_response(true, tip_daa, &report))
    }

    async fn submit_palw_model_registration_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitPalwModelRegistrationRequest,
    ) -> RpcResult<SubmitPalwModelRegistrationResponse> {
        let Some(bundle) = palw_v2_bundle(&self.config.params) else {
            return Ok(SubmitPalwModelRegistrationResponse::default());
        };
        let session = self.consensus_manager.consensus().unguarded_session();
        let tip_daa = session.get_virtual_daa_score();
        let object = if request.object_hex.trim().is_empty() {
            None
        } else {
            Some(decode_class_registered_hex(&request.object_hex)?)
        };
        let mut checks = Vec::new();
        let mut registration = RpcPalwModelRegistration { constructed: object.is_some(), ..Default::default() };
        if let Some(object) = &object {
            let bytes = borsh::to_vec(object).unwrap_or_default();
            registration.object_id =
                kaspa_consensus_core::palw_model_registration_v1::palw_registration_object_id_v1(&bytes).to_string();
            let (report, class_row) = self.palw_model_preflight_report(session.clone(), bundle, object, tip_daa).await?;
            registration.class_id = report.class_id.to_string();
            registration.processor_verdict = report.processor_verdict.clone();
            registration.reject_code = report.reject_code.clone();
            checks = report.checks.iter().map(rpc_preflight_check).collect();
            if let Some(row) = class_row {
                fill_registration_from_class(&mut registration, &row);
            }
        }
        if !request.transaction_id.trim().is_empty() {
            self.fill_registration_from_tx(&mut registration, &request.transaction_id).await?;
        }
        finalize_registration_state(&mut registration);
        Ok(SubmitPalwModelRegistrationResponse { available: true, tip_daa, registration, checks })
    }

    async fn get_palw_model_registration_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelRegistrationStatusRequest,
    ) -> RpcResult<GetPalwModelRegistrationStatusResponse> {
        if palw_v2_bundle(&self.config.params).is_none() {
            return Ok(GetPalwModelRegistrationStatusResponse::default());
        }
        let session = self.consensus_manager.consensus().unguarded_session();
        let tip_daa = session.get_virtual_daa_score();
        let mut registration = RpcPalwModelRegistration {
            object_id: request.object_id.clone(),
            class_id: request.class_id.clone(),
            transaction_id: request.transaction_id.clone(),
            constructed: !request.object_id.trim().is_empty(),
            ..Default::default()
        };
        if !request.class_id.trim().is_empty() {
            let class_id = parse_class_id_or_alias(&request.class_id)?;
            registration.class_id = class_id.to_string();
            let rows = session.clone().spawn_blocking(|c| c.palw_v2_class_table()).await;
            if let Some(row) = rows.into_iter().find(|r| r.class_id == class_id) {
                fill_registration_from_class(&mut registration, &row);
            }
            if let Some(read) = session.clone().spawn_blocking(|c| c.palw_model_registry_v1()).await {
                if let Some(class) = read.classes.iter().find(|c| c.class_id == class_id) {
                    registration.registry_state = class.row.as_ref().map(|r| format!("{:?}", r.state)).unwrap_or_else(|| "Legacy".into());
                    if class.row.is_some() {
                        registration.folded = true;
                        registration.included = true;
                        registration.accepted = true;
                        registration.submitted = true;
                        registration.constructed = true;
                    }
                }
            }
        }
        if !request.transaction_id.trim().is_empty() {
            self.fill_registration_from_tx(&mut registration, &request.transaction_id).await?;
        }
        finalize_registration_state(&mut registration);
        let found = registration.folded || registration.included || registration.accepted || registration.submitted || registration.constructed;
        if !found {
            registration.reject_code = kaspa_consensus_core::palw_model_registration_v1::PalwModelRegistrationCodeV1::RegistrationNotIncluded
                .code()
                .to_string();
            registration.processor_verdict = registration.reject_code.clone();
        }
        Ok(GetPalwModelRegistrationStatusResponse { available: true, tip_daa, found, registration })
    }

    async fn get_palw_model_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelRequest,
    ) -> RpcResult<GetPalwModelResponse> {
        if palw_v2_bundle(&self.config.params).is_none() {
            return Ok(GetPalwModelResponse::default());
        }
        let class_id = parse_class_id_or_alias(&request.class_id)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let tip_daa = session.get_virtual_daa_score();
        let rows = session.clone().spawn_blocking(|c| c.palw_v2_class_table()).await;
        let Some(row) = rows.into_iter().find(|r| r.class_id == class_id) else {
            return Ok(GetPalwModelResponse { available: true, tip_daa, found: false, class_id: class_id.to_string(), ..Default::default() });
        };
        let n_ctx = session
            .clone()
            .spawn_blocking(move |c| c.palw_registered_class_carriage_v1(class_id).map(|(profile, _)| profile.n_ctx))
            .await
            .unwrap_or(0);
        let registry = session.clone().spawn_blocking(|c| c.palw_model_registry_v1()).await;
        let panel = session.clone().spawn_blocking(|c| c.palw_panel_network_view_v1()).await;
        let class_reg = registry.as_ref().and_then(|r| r.classes.iter().find(|c| c.class_id == class_id));
        let panel_class = panel.as_ref().and_then(|v| v.classes.iter().find(|c| c.class_id == class_id));
        let families = session.clone().spawn_blocking(|c| c.palw_certified_families_v1()).await;
        let certified_family = session
            .spawn_blocking(move |c| {
                let Some((profile, _)) = c.palw_registered_class_carriage_v1(class_id) else { return String::new() };
                let reachable = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&profile);
                families
                    .into_iter()
                    .find(|(lane, _, record)| {
                        *lane == kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt
                            && reachable.is_subset(&record.family.kernel_ids)
                    })
                    .map(|(_, digest, _)| digest.to_string())
                    .unwrap_or_default()
            })
            .await;
        Ok(GetPalwModelResponse {
            available: true,
            tip_daa,
            found: true,
            class_id: class_id.to_string(),
            model_name: panel_class.map(|c| c.model_name.clone()).unwrap_or_default(),
            n_ctx,
            artifact_root: row.artifact_root.to_string(),
            class_status: row.status,
            registry_state: class_reg
                .and_then(|c| c.row.as_ref().map(|r| format!("{:?}", r.state)))
                .unwrap_or_else(|| "Legacy".into()),
            ready_seats: class_reg.map(|c| c.ready_seats_now).or_else(|| panel_class.map(|c| c.ready_seats)).unwrap_or(0),
            required_ready_seats: class_reg.and_then(|c| c.row.as_ref().map(|r| r.profile.required_ready_seats)).or_else(|| panel_class.map(|c| c.required_ready_seats)).unwrap_or(0),
            inflight_claims: class_reg.map(|c| c.inflight_now).or_else(|| panel_class.map(|c| c.inflight_claims)).unwrap_or(0),
            admission_permille: class_reg
                .and_then(|c| c.row.as_ref().map(|r| r.admission_milli as u32))
                .or_else(|| panel_class.map(|c| c.admission_permille as u32))
                .unwrap_or(0),
            share_permille: row.share_permille.unwrap_or(0),
            certified_family,
            fence_active: registry.as_ref().map(|r| r.active).unwrap_or(false),
            reason: class_reg.map(|c| c.reason.clone()).unwrap_or_default(),
        })
    }

    async fn get_palw_model_readiness_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelReadinessRequest,
    ) -> RpcResult<GetPalwModelReadinessResponse> {
        if palw_v2_bundle(&self.config.params).is_none() {
            return Ok(GetPalwModelReadinessResponse::default());
        }
        let class_id = parse_class_id_or_alias(&request.class_id)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let Some(read) = session.spawn_blocking(|c| c.palw_model_registry_v1()).await else {
            return Ok(GetPalwModelReadinessResponse { available: true, ..Default::default() });
        };
        let class = read.classes.iter().find(|c| c.class_id == class_id);
        let max_age = read.globals.map(|g| g.readiness_probe_max_age_spans as u64).unwrap_or(0).saturating_mul(read.span_daa.max(1));
        let seats = read
            .readiness
            .iter()
            .filter(|r| r.class_id == class_id)
            .map(|r| {
                let bond = read.bonds.iter().find(|b| b.bond == r.bond);
                let ready = r.fresh && r.not_ready_reason.is_empty();
                RpcPalwModelSeatReadiness {
                    seat_id: kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(r.bond),
                    bond_txid: r.bond.0.transaction_id.to_string(),
                    bond_index: r.bond.0.index,
                    proved_daa: r.row.proved_daa,
                    proved_span: r.row.proved_span,
                    expires_daa: r.row.proved_daa.saturating_add(max_age),
                    fresh: r.fresh,
                    ready,
                    collateral_sompi: bond.map(|b| b.free_collateral_sompi.min(u128::from(u64::MAX)) as u64).unwrap_or(0),
                    needed_collateral_sompi: bond.map(|b| b.needed_collateral_sompi.min(u128::from(u64::MAX)) as u64).unwrap_or(0),
                    not_ready_reason: if ready {
                        String::new()
                    } else if r.not_ready_reason.is_empty() {
                        kaspa_consensus_core::palw_model_registration_v1::PalwModelRegistrationCodeV1::ReadySeatsInsufficient
                            .code()
                            .to_string()
                    } else {
                        r.not_ready_reason.clone()
                    },
                }
            })
            .collect();
        Ok(GetPalwModelReadinessResponse {
            available: true,
            tip_daa: read.tip_daa,
            found: class.is_some(),
            class_id: class_id.to_string(),
            registry_state: class.and_then(|c| c.row.as_ref().map(|r| format!("{:?}", r.state))).unwrap_or_else(|| "Legacy".into()),
            ready_seats: class.map(|c| c.ready_seats_now).unwrap_or(0),
            required_ready_seats: class.and_then(|c| c.row.as_ref().map(|r| r.profile.required_ready_seats)).unwrap_or(0),
            seats,
        })
    }

    async fn get_palw_model_admission_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetPalwModelAdmissionRequest,
    ) -> RpcResult<GetPalwModelAdmissionResponse> {
        let pre = self
            .get_palw_model_preflight_call(
                connection,
                GetPalwModelPreflightRequest { object_hex: request.object_hex, class_id: request.class_id.clone() },
            )
            .await?;
        Ok(GetPalwModelAdmissionResponse {
            available: pre.available,
            tip_daa: pre.tip_daa,
            class_id: if pre.class_id.is_empty() { request.class_id } else { pre.class_id },
            admissible: pre.admissible,
            processor_verdict: pre.processor_verdict,
            reject_code: pre.reject_code,
            checks: pre.checks,
        })
    }

    async fn get_palw_model_certification_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwModelCertificationRequest,
    ) -> RpcResult<GetPalwModelCertificationResponse> {
        if palw_v2_bundle(&self.config.params).is_none() {
            return Ok(GetPalwModelCertificationResponse::default());
        }
        let class_id = parse_class_id_or_alias(&request.class_id)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let tip_daa = session.get_virtual_daa_score();
        let Some((profile, _)) = session.clone().spawn_blocking(move |c| c.palw_registered_class_carriage_v1(class_id)).await else {
            return Ok(GetPalwModelCertificationResponse {
                available: true,
                tip_daa,
                found: false,
                class_id: class_id.to_string(),
                ..Default::default()
            });
        };
        let reachable = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&profile);
        let families = session.spawn_blocking(|c| c.palw_certified_families_v1()).await;
        let families: Vec<RpcPalwModelCertifiedFamily> = families
            .into_iter()
            .map(|(lane, digest, record)| {
                let covers = reachable.is_subset(&record.family.kernel_ids);
                RpcPalwModelCertifiedFamily {
                    lane: match lane {
                        kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt => "attempt".into(),
                        kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt => "free_prompt".into(),
                    },
                    digest: digest.to_string(),
                    covers,
                }
            })
            .collect();
        let end_to_end_certified = families.iter().any(|f| f.lane == "attempt" && f.covers);
        Ok(GetPalwModelCertificationResponse {
            available: true,
            tip_daa,
            found: true,
            class_id: class_id.to_string(),
            end_to_end_certified,
            families,
        })
    }

    async fn get_palw_producer_facts_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetPalwProducerFactsRequest,
    ) -> RpcResult<GetPalwProducerFactsResponse> {
        // **ADR-0042 Decision 6, on the wire — what makes third-party mining possible.**
        //
        // `available: false` on any network that is not `ConsensusV2` and on one that does not
        // know the class: both are honest answers, not errors. The facts are handed over DERIVED
        // (ADR-0046) — the class target, the pwu it implies, the artifact root, the registered
        // key, the operator id and the exposure room — because exposing the ingredients would
        // give every producer an independent chance to disagree with admission.
        // **A malformed request is an ERROR, not an answer.** These two arms used to return the
        // default response, whose `available: false` this struct documents as "not a ConsensusV2
        // network" — so a caller who fat-fingered a class id was told the chain was something it is
        // not. Three different failures shared one indistinguishable reply.
        //
        // **An EMPTY class id is a legal request, and it asks only for the locked-bond set**
        // (audit3 H3). A wallet has to know which of its outputs are bonded collateral before it
        // selects inputs, and it has no class id to offer — `get_stake_bonds` reads the DNS overlay
        // store only, so a PALW producer's collateral was invisible to the very selector that
        // exists to skip it. Anything else non-empty and unparseable is still an error, because a
        // fat-fingered class id must not read as "this chain does not have that class".
        // **Everything the CALLER sent is parsed before this node reads a single byte of chain
        // state** (mainnet audit M-5). The two session reads below are full PALW-state
        // materializations, and they used to run above the parses — so a caller who fat-fingered
        // a class id was told '…is not a 128-hex Hash64' only after the node had decoded the
        // whole carriage, rebuilt both indices, walked both consistency checks and recomputed the
        // root for them. A malformed request must be free. The bond id is still parsed only when
        // a class id was named, exactly as before: an empty class id has never reached it.
        let class_id = if request.class_id.is_empty() {
            None
        } else {
            Some(
                request
                    .class_id
                    .parse::<kaspa_hashes::Hash64>()
                    .map_err(|_| RpcError::General(format!("class id '{}' is not a 128-hex Hash64", request.class_id)))?,
            )
        };
        let bond = if class_id.is_some() && request.with_bond {
            let transaction_id = request.bond_transaction_id.parse::<kaspa_consensus_core::tx::TransactionId>().map_err(|_| {
                RpcError::General(format!("bond transaction id '{}' is not a 128-hex transaction id", request.bond_transaction_id))
            })?;
            Some(kaspa_consensus_core::tx::TransactionOutpoint { transaction_id, index: request.bond_index })
        } else {
            None
        };
        // ADR-0084 Decision 5: where this node's panel serves from, empty on a node with no panel.
        let palw_retention_dir = self.flow_context.palw_retention_dir().map(|d| d.display().to_string()).unwrap_or_default();
        // **One blocking hop, one tip.** Two reasons this is not two calls. First, these are
        // synchronous consensus reads and this is an `async fn`: run inline they occupy an RPC
        // reactor thread rather than a blocking worker, which is what makes an unauthenticated
        // caller's cost land on the runtime that also carries block relay and IBD. Second, the two
        // answers are assembled into one response, and this file already records what an answer
        // built from two chain points costs — see `fp_decode_rules_armed` below.
        let session = self.consensus_manager.consensus().unguarded_session();
        let (locked_outpoints, facts, class_profile) = session
            .spawn_blocking(move |c| {
                let locked = c.palw_locked_bond_outpoints_v2();
                let facts = class_id.and_then(|class_id| c.palw_producer_facts_v2(class_id, bond));
                // ADR-0118 Decision 3: the class's registered profile says which form its jobs
                // commit their prompt ids in (a held class: Merkle on every network).
                let class_profile = class_id.and_then(|class_id| c.palw_registered_class_carriage_v1(class_id)).map(|(p, _)| p);
                (locked, facts, class_profile)
            })
            .await;
        // Consensus-locked collateral AND this node's own reserved funding outpoints, in one list
        // (audit3 H3 + H12). A wallet that reads only the first spends the panel's fee outpoint.
        let locked_bond_outpoints: Vec<String> = locked_outpoints
            .into_iter()
            .chain(self.flow_context.palw_reserved_outpoints())
            .map(|o| format!("{}:{}", o.transaction_id, o.index))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if class_id.is_none() {
            return Ok(GetPalwProducerFactsResponse {
                available: !locked_bond_outpoints.is_empty(),
                locked_bond_outpoints,
                palw_retention_dir,
                ..Default::default()
            });
        }
        let Some(facts) = facts else {
            return Ok(GetPalwProducerFactsResponse::default());
        };
        // **The free-prompt lane's price comes from the bundle this node runs, not from the
        // caller's guess** (ADR-0077 Decision 3). `quanta_per_canonical_job` and the per-receipt
        // cap are genesis constants of the network, not chain state, so they are read off the
        // config the node booted with — the same two numbers `FreePromptCommitted` uses to turn
        // `work_leaves` into quanta and therefore into the exposure it reserves. A gateway that
        // hardcoded "an eighth" (as the shipped one did) would size its own exposure room by a
        // constant the chain owns. Zero on a network that prices no free-prompt lane, which is an
        // honest answer: a commitment there enters no state.
        let (fp_quanta_per_canonical_job, fp_max_quanta_per_receipt) = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                (bundle.freeprompt.quanta_per_canonical_job(), bundle.freeprompt.max_quanta_per_receipt())
            }
            _ => (0, 0),
        };
        let mut response = GetPalwProducerFactsResponse {
            available: true,
            fp_certified: facts.fp_certified,
            fp_quanta_per_canonical_job,
            fp_max_quanta_per_receipt,
            // **ADR-0082 Decisions 10/11's fence, at the point these facts were read.** The
            // fence lives on `Params`, not in chain state, so it is read off the config this node
            // booted with — and at `facts.daa_score`, the CANDIDATE's score, because every other
            // fact in this response was derived at that point and a readiness answer assembled
            // from two chain points is the defect this file already records for the free-prompt
            // price. `palw_fp_decode_rules_active_at` folds in the ConsensusV2 mode condition, so
            // a hash-only network answers false without a second check here.
            fp_decode_rules_armed: self.config.params.palw_fp_decode_rules_active_at(facts.daa_score),
            chain_point: facts.chain_point.to_string(),
            daa_score: facts.daa_score,
            class_id: facts.class_id.to_string(),
            artifact_root: facts.artifact_root.to_string(),
            class_target: facts.class_target.to_string(),
            pwu: facts.pwu,
            is_base_class: facts.is_base_class,
            min_trace_retention_daa: facts.min_trace_retention_daa,
            epoch_index: facts.epoch_index,
            epoch_budget_blocks: facts.epoch_budget_blocks,
            epoch_produced_blocks: facts.epoch_produced_blocks,
            locked_bond_outpoints,
            palw_retention_dir,
            // Both fences at the CANDIDATE's score, for the reason `fp_decode_rules_armed` is; the
            // form is genesis-only so the height cannot matter, and it is read the same way anyway.
            panel_da_armed: self.config.params.palw_panel_da_at(facts.daa_score),
            // ADR-0096 Decision 8's fence at the candidate's score, read the same way.
            fp_decode_constraint_armed: self.config.params.palw_fp_decode_constraint_active_at(facts.daa_score),
            prompt_ids_merkle: self.config.params.palw_prompt_ids_form_at(facts.daa_score)
                == kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            // ADR-0118 Decision 3: THIS class's form — the network's, or Merkle for a class under
            // the held regime whatever the network's. A class this node holds no registration row
            // for (a genesis row) is not a held one on a network minted flat: the gate admits a
            // held class only past the fence, by a registration, which is what writes the row.
            class_prompt_ids_merkle: {
                let network = self.config.params.palw_prompt_ids_form_at(facts.daa_score);
                class_profile.as_ref().map_or(network, |profile| {
                    kaspa_consensus_core::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(network, profile)
                }) == kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1
            },
            ..Default::default()
        };
        if let Some(bond_facts) = facts.bond.as_ref() {
            response.bond_known = true;
            response.bond_registered_pubkey = faster_hex::hex_string(&bond_facts.registered_pubkey);
            response.bond_operator_id = bond_facts.operator_id.to_string();
            response.bond_collateral = bond_facts.collateral;
            response.bond_reserved_exposure = bond_facts.reserved_exposure.to_string();
            response.bond_exposure_ceiling = bond_facts.exposure_ceiling.to_string();
            response.bond_claim_exposure = bond_facts.claim_exposure.to_string();
        }
        // **The verdict is computed for every request, not only for bonds that exist.**
        //
        // This call used to live inside the `if let` above, so a caller naming a bond the chain has
        // never registered got `not_ready_reason: ""` — which this struct documents as "this bond
        // may produce now". `ready_to_produce`'s FIRST line is the arm that answers that case, it is
        // directly unit-tested, and the one RPC that exists to serve the verdict could not reach it.
        //
        // Empty must keep exactly one meaning ("ready"), so a request that named no bond gets a
        // sentence of its own rather than an empty string: readiness is a property of a bond, and
        // not asking about one is not the same as asking and being told yes.
        //
        // The verdict still comes from `ready_to_produce` rather than being re-derived here — the
        // RPC and the producer must not be able to disagree about what "ready" means. Its key check
        // is against the caller's own key, which this server does not hold, so the answer is given
        // for the key the bond registered; with no bond there is no such key and the first arm
        // fires before the comparison.
        response.not_ready_reason = if !request.with_bond {
            "no bond was named in this request — readiness is a property of a bond".to_string()
        } else {
            let key = facts.bond.as_ref().map(|b| b.registered_pubkey.as_slice()).unwrap_or(&[]);
            match (facts.ready_to_produce(key), facts.class_admission_refusal.as_deref()) {
                // The sentence first — the CLI matches on it — then the gate's own words.
                (Err(why), Some(detail)) if why == kaspa_consensus_core::palw_producer_v2::PALW_NOT_READY_CLASS_NOT_ADMITTING_V2 => {
                    format!("{why} [{detail}]")
                }
                (verdict, _) => verdict.err().unwrap_or_default().to_string(),
            }
        };
        Ok(response)
    }

    async fn get_token_supply_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetTokenSupplyRequest,
    ) -> RpcResult<GetTokenSupplyResponse> {
        // The token overlay is removed: `available: false`, kept for wire compatibility.
        Ok(GetTokenSupplyResponse::default())
    }

    async fn get_token_emission_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetTokenEmissionInfoRequest,
    ) -> RpcResult<GetTokenEmissionInfoResponse> {
        // The token overlay is removed: `available: false`, kept for wire compatibility.
        Ok(GetTokenEmissionInfoResponse::default())
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
                // The VLT activation state machine and its gauges are removed (VLT voting weight is
                // gone). The fields stay for wire compatibility and answer what a pre-v4 payload
                // decodes to: the empty status.
                vlt_state: "pre_shadow".to_string(),
                vlt_shadow_active: false,
                vlt_weight_fence_reached: false,
                vlt_finality_active: false,
                vlt_total_weight: "0".to_string(),
                vlt_quorum_weight: "0".to_string(),
                vlt_snapshot_epoch: 0,
                vlt_snapshot_root: String::new(),
                vlt_gauges_daa_score: 0,
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
            if let Some((anchor, dns_confirmed)) = anchor_info
                && anchor != kaspa_hashes::Hash64::default()
            {
                response.block_is_confirmed_anchor = hash == anchor;
                response.block_is_dns_final = dns_confirmed
                    && (response.block_is_confirmed_anchor || session.async_is_chain_ancestor_of(hash, anchor).await.unwrap_or(false));
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
        if !hex_str.len().is_multiple_of(2) {
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

        // The template's own predicate (`bridge_finality_is_fresh` in the virtual processor), read
        // the same way: blue score, beyond the anchor's healthy distance below the sink. A claim
        // this accepts is one the template will carry, and the reverse.
        let dns_confirmation = session.async_get_dns_confirmation().await;
        let sink_blue_score = session.async_get_sink_blue_score().await;
        let (bridge_finality_fresh, anchor_distance) = match (dns_confirmation.as_ref(), self.config.params.dns_params.as_ref()) {
            (Some(c), Some(dns)) => {
                let anchor_blue_score = if c.last_dns_confirmed_anchor == Default::default() {
                    None
                } else {
                    session.async_get_header(c.last_dns_confirmed_anchor).await.ok().map(|header| header.blue_score)
                };
                (
                    kaspa_consensus_core::dns_finality::dns_finality_fresh_for_bridge(
                        c.dns_confirmed,
                        c.last_dns_confirmed_anchor,
                        anchor_blue_score,
                        sink_blue_score,
                        dns,
                    ),
                    anchor_blue_score.map(|anchor| {
                        (
                            sink_blue_score.saturating_sub(anchor),
                            kaspa_consensus_core::dns_finality::dns_bridge_max_anchor_distance_blue_score(dns),
                        )
                    }),
                )
            }
            _ => (false, None),
        };
        // ADR-0109 Decision 2: under `Label` (the default) a stale anchor refuses nothing here — the
        // claim waits in the queue like any other; under `Pause` the pre-ADR refusal stands.
        if !bridge_finality_fresh
            && self.config.evm_bridge_finality_effective() == kaspa_consensus_core::evm::EvmBridgeFinalityPolicy::Pause
        {
            return Err(RpcError::RpcSubsystem(format!(
                "EVM bridge is paused: DNS finality is unconfirmed or stale at sink daa {sink_daa} ({}); retry after validators advance a fresh DNS-confirmed anchor",
                match (dns_confirmation.as_ref().map(|c| c.dns_confirmed), anchor_distance) {
                    (Some(false), _) => "DNS finality is not confirmed".to_string(),
                    (_, Some((distance, bound))) =>
                        format!("the confirmed anchor is {distance} blue below the sink; the bridge allows {bound}"),
                    _ => "no DNS-confirmed anchor this node can read".to_string(),
                }
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

    async fn get_attestation_quality_deficits_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetAttestationQualityDeficitsRequest,
    ) -> RpcResult<GetAttestationQualityDeficitsResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let deficits = session
            .async_get_attestation_quality_deficits()
            .await
            .into_iter()
            .map(|d| RpcAttestationQualityDeficit {
                epoch: d.epoch,
                target_hash: d.target_hash.to_string(),
                target_daa_score: d.target_daa_score,
                included_stake: d.included_stake,
                expected_stake: d.expected_stake,
                required_stake: d.required_stake,
                required_stake_delta: d.required_stake_delta,
                quality_floor_bps: d.quality_floor_bps,
                health: d.health as u32,
            })
            .collect();
        Ok(GetAttestationQualityDeficitsResponse { deficits })
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

    async fn get_validator_attestation_targets_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetValidatorAttestationTargetsRequest,
    ) -> RpcResult<GetValidatorAttestationTargetsResponse> {
        // kaspa-pq DNS v3 (batch): every ready attestation target for `request.bond_outpoint`
        // from `from_epoch` (ascending, capped) so an external validator that fell behind can sign
        // every missed epoch in one poll. A malformed outpoint is a request error; empty when the
        // overlay is off or none are ready.
        const MAX_TARGETS: u32 = 64;
        let bond_outpoint = parse_bond_outpoint(&request.bond_outpoint)?;
        let limit = request.limit.min(MAX_TARGETS) as usize;
        let session = self.consensus_manager.consensus().unguarded_session();
        let targets = session
            .async_get_validator_attestation_targets(bond_outpoint, request.from_epoch, limit)
            .await
            .into_iter()
            .map(|t| RpcValidatorAttestationTarget {
                epoch: t.epoch,
                target_hash: t.target_hash.to_string(),
                target_daa_score: t.target_daa_score,
                validator_set_commitment: t.validator_set_commitment.to_string(),
                message: t.message.to_string(),
            })
            .collect();
        Ok(GetValidatorAttestationTargetsResponse { targets })
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

    async fn get_stake_bonds_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetStakeBondsRequest,
    ) -> RpcResult<GetStakeBondsResponse> {
        // kaspa-pq: paged, filtered enumeration of the StakeBonds overlay store.
        // The store is outpoint-keyed with no owner index, so the owner filter is a
        // full scan; the page is bounded by `limit` and walked with an outpoint
        // cursor. Primary use: an owner recovering the outpoint(s) of bonds they
        // funded (the only key a StakeUnbondRequest binds to).
        let owner_pubkey_hash = match &request.owner_pubkey_hash {
            Some(h) => Some(
                h.parse::<kaspa_hashes::Hash64>()
                    .map_err(|_| RpcError::General(format!("owner_pubkey_hash '{h}' is not a valid 64-byte Hash64")))?,
            ),
            None => None,
        };
        let status_in = match &request.status_in {
            Some(list) => {
                let mut statuses = Vec::with_capacity(list.len());
                for s in list {
                    statuses.push(parse_bond_status(s)?);
                }
                Some(statuses)
            }
            None => None,
        };
        let cursor = match &request.cursor {
            Some(c) => Some(parse_bond_outpoint(c)?),
            None => None,
        };
        let query = kaspa_consensus_core::dns_finality::StakeBondQuery {
            owner_pubkey_hash,
            status_in,
            cursor,
            limit: request.limit as usize,
            pov_daa_score: request.pov_daa_score,
        };

        let session = self.consensus_manager.consensus().unguarded_session();
        let page = session.async_get_stake_bonds(query).await;
        let bonds = page
            .bonds
            .into_iter()
            .map(|r| {
                let effective = kaspa_consensus_core::dns_finality::effective_bond_status(&r, page.pov_daa_score);
                RpcStakeBondEntry {
                    bond_outpoint: format!("{}:{}", r.bond_outpoint.transaction_id, r.bond_outpoint.index),
                    owner_pubkey_hash: r.owner_pubkey_hash.to_string(),
                    validator_id: r.validator_pubkey_hash.to_string(),
                    amount: r.amount,
                    activation_daa_score: r.activation_daa_score,
                    unbonding_period_blocks: r.unbonding_period_blocks,
                    unbond_request_daa_score: r.unbond_request_daa_score,
                    stored_status: bond_status_str(r.status).to_string(),
                    effective_status: bond_status_str(effective).to_string(),
                }
            })
            .collect();
        let next_cursor = page.next_cursor.map(|o| format!("{}:{}", o.transaction_id, o.index));
        Ok(GetStakeBondsResponse { bonds, next_cursor, pov_daa_score: page.pov_daa_score })
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
            if c.len().is_multiple_of(2) {
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
        // Same definition as getSyncStatus and getInfo. This one is what `kaspa-pq-validator` polls
        // before it will attest, and it used to omit the transitional-IBD check the other two had —
        // so a node mid-IBD whose sink had advanced could report synced to the one caller whose
        // reaction is to start signing.
        let is_synced = self.is_node_synced(&session, sink_daa_score_timestamp).await;
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
        let is_synced = self.is_node_synced(&session, sink_daa_score_timestamp).await;
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

fn palw_v2_bundle(params: &kaspa_consensus_core::config::params::Params) -> Option<&kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2> {
    match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle),
        _ => None,
    }
}

fn decode_class_registered_hex(hex: &str) -> RpcResult<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
    let hex = hex.trim();
    if hex.is_empty() {
        return Err(RpcError::General("objectHex is empty".into()));
    }
    if hex.len() % 2 != 0 {
        return Err(RpcError::General("objectHex is not even-length hex".into()));
    }
    let mut bytes = vec![0u8; hex.len() / 2];
    faster_hex::hex_decode(hex.as_bytes(), &mut bytes).map_err(|e| RpcError::General(format!("objectHex: {e}")))?;
    let object: kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 =
        borsh::from_slice(&bytes).map_err(|e| RpcError::General(format!("objectHex is not a ClassRegistered: {e}")))?;
    if !matches!(object, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { .. }) {
        return Err(RpcError::General("objectHex is not a ClassRegistered".into()));
    }
    Ok(object)
}

fn rpc_preflight_check(check: &kaspa_consensus_core::palw_model_registration_v1::PalwModelPreflightCheckV1) -> RpcPalwModelPreflightCheck {
    RpcPalwModelPreflightCheck { code: check.code.clone(), ok: check.ok, message: check.message.clone() }
}

fn rpc_preflight_response(
    available: bool,
    tip_daa: u64,
    report: &kaspa_consensus_core::palw_model_registration_v1::PalwModelPreflightReportV1,
) -> GetPalwModelPreflightResponse {
    GetPalwModelPreflightResponse {
        available,
        tip_daa,
        class_id: report.class_id.to_string(),
        artifact_root: report.artifact_root.to_string(),
        n_ctx: report.n_ctx,
        layer_count: u32::from(report.layer_count),
        admissible: report.admissible,
        processor_verdict: report.processor_verdict.clone(),
        reject_code: report.reject_code.clone(),
        checks: report.checks.iter().map(rpc_preflight_check).collect(),
    }
}

fn fill_registration_from_class(registration: &mut RpcPalwModelRegistration, row: &kaspa_consensus_core::palw_state_v2::PalwClassRowV2) {
    registration.class_id = row.class_id.to_string();
    registration.constructed = true;
    registration.submitted = true;
    registration.accepted = true;
    registration.included = true;
    registration.folded = true;
    registration.included_daa = row.registered_daa;
    registration.mempool_accepted = false;
    registration.reject_code.clear();
    if registration.processor_verdict.is_empty() {
        registration.processor_verdict = kaspa_consensus_core::palw_model_registration_v1::PalwModelRegistrationCodeV1::AdmissionOk
            .code()
            .to_string();
    }
}

fn finalize_registration_state(registration: &mut RpcPalwModelRegistration) {
    use kaspa_consensus_core::palw_model_registration_v1::PalwModelRegistrationStageV1 as Stage;
    let stage = if registration.folded {
        Stage::Folded
    } else if registration.included {
        Stage::Included
    } else if registration.accepted {
        Stage::Accepted
    } else if registration.submitted {
        Stage::Submitted
    } else if registration.constructed {
        Stage::Constructed
    } else {
        Stage::Constructed
    };
    registration.submission_state = stage.name().to_string();
    if registration.folded {
        registration.included = true;
        registration.accepted = true;
        registration.submitted = true;
        registration.constructed = true;
    }
}

/// A 128-hex `Hash64` off the wire, or an error naming the field — a malformed id is a request
/// error, never an absence (the integration test pins the difference).
fn parse_hash64(text: &str, what: &str) -> RpcResult<kaspa_hashes::Hash64> {
    text.parse::<kaspa_hashes::Hash64>().map_err(|_| RpcError::General(format!("{what} '{text}' is not a 128-hex Hash64")))
}

fn parse_class_id_or_alias(raw: &str) -> RpcResult<kaspa_hashes::Hash64> {
    kaspa_consensus_core::palw_panel_view_v1::palw_parse_class_alias_v1(raw).map_err(RpcError::General)
}

fn rpc_hold(reason: kaspa_consensus_core::palw_panel_view_v1::PalwPanelHoldReasonV1) -> RpcPalwPanelHoldReason {
    RpcPalwPanelHoldReason { code: reason.code().to_string(), message: reason.message().to_string() }
}

fn rpc_palw_class_panel_status(
    row: &kaspa_consensus_core::palw_panel_view_v1::PalwClassPanelViewV1,
    holds_local: u32,
) -> RpcPalwClassPanelStatus {
    RpcPalwClassPanelStatus {
        class_id: row.class_id.to_string(),
        model_name: row.model_name.clone(),
        registry_state: row.registry_state.clone(),
        bonded_seats: row.bonded_seats,
        ready_seats: row.ready_seats,
        required_ready_seats: row.required_ready_seats,
        selected_panel_seats: row.selected_panel_seats,
        valid_receipt_seats: row.valid_receipt_seats,
        panel_size: row.geometry.panel_size,
        receipt_quorum: row.geometry.receipt_quorum,
        full_seats_per_panel: row.geometry.full_seats_per_panel,
        partial_seats_per_panel: row.geometry.partial_seats_per_panel,
        segment_count: row.geometry.segment_count,
        inflight_claims: row.inflight_claims,
        active_assignments: row.active_assignments,
        admission_permille: row.admission_permille,
        verification_mode: row.verification.name.to_string(),
        s1_active: row.verification.s1_active,
        s1_scheduled_daa: row.verification.s1_scheduled_daa,
        s3_active: row.verification.s3_active,
        s3_scheduled_daa: row.verification.s3_scheduled_daa,
        s2_active: row.verification.s2_active,
        s2_scheduled_daa: row.verification.s2_scheduled_daa,
        holds_local,
        missing: row
            .missing
            .iter()
            .map(|(reason, n)| RpcPalwPanelHoldReasonCount {
                code: reason.code().to_string(),
                message: reason.message().to_string(),
                seats: *n,
            })
            .collect(),
    }
}

fn rpc_palw_panel_seat(row: kaspa_consensus_core::palw_panel_view_v1::PalwPanelSeatViewV1) -> RpcPalwPanelSeat {
    let id = kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(row.seat_id);
    RpcPalwPanelSeat {
        seat_id: id.clone(),
        bond_outpoint: id,
        class_id: row.class_id.to_string(),
        ready: row.ready,
        eligible: row.eligible,
        readiness_version: row.readiness_version,
        readiness_proved_daa: row.readiness_proved_daa,
        readiness_expires_daa: row.readiness_expires_daa,
        collateral_available: row.collateral_available.to_string(),
        collateral_locked: row.collateral_locked.to_string(),
        assigned: row.assigned,
        hold: row.hold.map(rpc_hold),
    }
}

fn rpc_palw_panel_assignment(row: kaspa_consensus_core::palw_panel_view_v1::PalwPanelAssignmentViewV1) -> RpcPalwPanelAssignment {
    RpcPalwPanelAssignment {
        claim_id: row.claim_id.to_string(),
        class_id: row.class_id.to_string(),
        licensed_state: row.licensed_state.to_string(),
        deadline_daa: row.deadline_daa,
        coverage_mask: row.coverage_mask,
        full_seat: kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(row.full_seat),
        valid_receipt_seats: row.valid_receipt_seats,
        selected_panel_seats: row.selected_panel_seats,
        seats: row
            .seats
            .into_iter()
            .map(|s| RpcPalwPanelAssignmentSeat {
                seat_id: kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(s.seat_id),
                seat_index: s.seat_index,
                full_seat: s.full_seat,
                segment_index: s.segment_index,
                mask: s.mask,
                receipt_status: s.receipt_status.to_string(),
                credited_daa: s.credited_daa,
            })
            .collect(),
    }
}

fn local_hold_or_chain(
    local: &kaspa_p2p_flows::flow_context::PalwLocalPanelClassV1,
    seat: Option<&kaspa_consensus_core::palw_panel_view_v1::PalwPanelSeatViewV1>,
) -> Option<RpcPalwPanelHoldReason> {
    if !local.hold_code.is_empty() {
        return Some(RpcPalwPanelHoldReason { code: local.hold_code.clone(), message: local.hold_message.clone() });
    }
    seat.and_then(|s| s.hold).map(rpc_hold)
}

/// ADR-0088 Decision 12: one line's row for the wire.
/// ADR-0095: the membership as a card — the tiers governing now, the weakening waiting out its
/// notice, and why the promise is silent when it is.
fn rpc_palw_model_benefits(read: &kaspa_consensus_core::api::PalwModelBenefitsReadV1) -> kaspa_rpc_core::RpcPalwModelBenefits {
    use kaspa_consensus_core::palw_model_benefits_v1::{PalwModelBenefitLapseV1, grant};
    fn tier(t: &kaspa_consensus_core::palw_model_benefits_v1::PalwModelBenefitTierV1) -> kaspa_rpc_core::RpcPalwModelBenefitTier {
        kaspa_rpc_core::RpcPalwModelBenefitTier {
            min_units: t.min_units,
            grants: t.grants,
            grant_names: grant::names_of(t.grants).into_iter().map(String::from).collect(),
            lead_daa: t.lead_daa,
            min_hold_daa: t.min_hold_daa,
            note: String::from_utf8_lossy(&t.note).to_string(),
        }
    }
    let (lapsed, lapse_daa) = match read.lapse {
        Some(PalwModelBenefitLapseV1::Expired { at_daa }) => (Some("expired".to_string()), Some(at_daa)),
        Some(PalwModelBenefitLapseV1::CadenceMissed { due_daa }) => (Some("cadenceMissed".to_string()), Some(due_daa)),
        None => (None, None),
    };
    kaspa_rpc_core::RpcPalwModelBenefits {
        tiers: read.in_effect.iter().map(tier).collect(),
        pending_tiers: read.row.pending.as_ref().map(|p| p.tiers.iter().map(tier).collect()).unwrap_or_default(),
        pending_effective_daa: read.row.pending.as_ref().map(|p| p.effective_daa),
        cadence_daa: read.row.cadence_daa,
        expires_daa: read.row.expires_daa,
        declared_daa: read.row.declared_daa,
        lapsed,
        lapse_daa,
        enforced_lead_daa: read.enforced_lead_daa,
    }
}

fn rpc_palw_model_line(row: &kaspa_consensus_core::api::PalwModelLineRowReadV1) -> RpcPalwModelLine {
    let line = &row.line;
    RpcPalwModelLine {
        line_id: row.line_id.to_string(),
        class_id: line.class_id.to_string(),
        has_row: row.has_row,
        owner: line.owner.map(|b| b.0.into()),
        owner_payout_payload: row.owner_payout_payload.map(|h| h.to_string()),
        developer: line.developer.map(|b| b.0.into()),
        developer_payout_payload: row.developer_payout_payload.map(|h| h.to_string()),
        maintainer: line.maintainer.map(|b| b.0.into()),
        maintainer_payout_payload: row.maintainer_payout_payload.map(|h| h.to_string()),
        name: String::from_utf8_lossy(&line.name).into_owned(),
        name_hex: faster_hex::hex_string(&line.name),
        founded_daa: line.founded_daa,
        current: line.current,
        previews: line.previews.clone(),
        versions_published: line.versions_published,
        contributor_permille_of_leg: line.contributor_permille_of_leg as u32,
        status: format!("{:?}", line.status),
        retired_daa: line.retired_daa,
    }
}

/// ADR-0088 Decision 12: one version's row for the wire, `in_force` judged at `tip_daa`.
fn rpc_palw_model_version(
    line_id: kaspa_hashes::Hash64,
    version: u32,
    row: &kaspa_consensus_core::palw_model_lines_v1::PalwModelVersionV1,
    tip_daa: u64,
) -> RpcPalwModelVersion {
    use kaspa_consensus_core::palw_model_lines_v1::PalwVersionStatusV1;
    let (status, until_daa) = match row.status {
        PalwVersionStatusV1::Current => ("Current", None),
        PalwVersionStatusV1::Preview => ("Preview", None),
        PalwVersionStatusV1::Superseded { until_daa } => ("Superseded", Some(until_daa)),
        PalwVersionStatusV1::Withdrawn => ("Withdrawn", None),
    };
    RpcPalwModelVersion {
        line_id: line_id.to_string(),
        version,
        root: row.root.to_string(),
        parent: row.parent,
        adopted_from: row.adopted_from.map(|h| h.to_string()),
        runtime_hash: row.runtime_hash.map(|h| h.to_string()),
        dataset_commitment: row.dataset_commitment.map(|h| h.to_string()),
        training_config_hash: row.training_config_hash.map(|h| h.to_string()),
        notes_hash: row.notes_hash.map(|h| h.to_string()),
        published_daa: row.published_daa,
        published_by: row.published_by.map(|b| b.0.into()),
        status: status.to_string(),
        until_daa,
        in_force: row.in_force_at(tip_daa),
        attempt_claims: row.usage.attempt_claims,
        fp_claims: row.usage.fp_claims,
        work_leaves: row.usage.work_leaves.to_string(),
        first_used_daa: row.usage.first_used_daa,
        last_used_daa: row.usage.last_used_daa,
    }
}

#[cfg(test)]
mod palw_class_context_tests {
    use super::*;

    struct Ledger;

    impl PalwClassLedgerProvider for Ledger {
        fn class_context(&self, class_id: kaspa_hashes::Hash64) -> Option<PalwClassLedgerContext> {
            (class_id != kaspa_hashes::Hash64::from_u64_word(3)).then(|| PalwClassLedgerContext {
                model_id: format!("model-{}", &class_id.to_string()[..8]),
                n_ctx: 4_096,
                canonical_prefill_tokens: 60,
                canonical_decode_tokens: 3,
                max_context_tokens: 512,
            })
        }
    }

    /// This build's Qwen3.6-35B-A3B row: a (7, 2) canonical job at `n_ctx` 8 — a footprint of 8, valid.
    fn declared() -> PalwRegisteredClassContext {
        PalwRegisteredClassContext { n_ctx: 8, canonical_prefill_tokens: 7, canonical_decode_tokens: 2, max_context_tokens: 8 }
    }

    /// **The chain's declaration outranks the build's ledger, the ledger names the model, and a class
    /// neither knows is zeros that say `unknown`** — never a ledger's numbers passed off as the
    /// chain's, and never zeros passed off as a context.
    #[test]
    fn a_class_context_is_the_chains_then_the_builds_then_unknown() {
        let (one, two, three) =
            (kaspa_hashes::Hash64::from_u64_word(1), kaspa_hashes::Hash64::from_u64_word(2), kaspa_hashes::Hash64::from_u64_word(3));
        let ledger: Arc<dyn PalwClassLedgerProvider> = Arc::new(Ledger);

        let registered = palw_class_context_row(one, Some(declared()), ledger.class_context(one));
        assert_eq!(registered.source, "chain_registration");
        assert_eq!(
            (registered.n_ctx, registered.canonical_prefill_tokens, registered.canonical_decode_tokens, registered.max_context_tokens),
            (8, 7, 2, 8),
            "the registered declaration's numbers, not the ledger's"
        );
        assert_eq!(
            registered.canonical_footprint_positions, 8,
            "(7, 2) caches 7 + 2 - 1 = 8 positions: the first token comes from the prefill's last logits, so it fits n_ctx 8"
        );
        assert!(registered.canonical_footprint_positions <= registered.n_ctx);
        assert_eq!(registered.model_id, ledger.class_context(one).unwrap().model_id, "the ledger names the model");
        assert_eq!(registered.class_id, one.to_string());

        let registered_unnamed = palw_class_context_row(three, Some(declared()), ledger.class_context(three));
        assert_eq!(
            (registered_unnamed.source.as_str(), registered_unnamed.model_id.as_str(), registered_unnamed.n_ctx),
            ("chain_registration", "", 8)
        );

        let built = palw_class_context_row(two, None, ledger.class_context(two));
        assert_eq!(built.source, "build_ledger");
        assert_eq!(
            (built.n_ctx, built.canonical_prefill_tokens, built.canonical_decode_tokens, built.max_context_tokens),
            (4_096, 60, 3, 512)
        );
        assert_eq!(built.canonical_footprint_positions, 62, "the ledger's row is footprinted by the same rule");
        assert!(!built.model_id.is_empty());

        let unknown = palw_class_context_row(three, None, ledger.class_context(three));
        assert_eq!(unknown.source, "unknown");
        assert_eq!(
            (unknown.n_ctx, unknown.canonical_footprint_positions, unknown.max_context_tokens, unknown.model_id.as_str()),
            (0, 0, 0, "")
        );
        let decode_free =
            PalwRegisteredClassContext { n_ctx: 8, canonical_prefill_tokens: 8, canonical_decode_tokens: 0, max_context_tokens: 8 };
        assert_eq!(
            palw_class_context_row(one, Some(decode_free), None).canonical_footprint_positions,
            8,
            "decode counts at least one"
        );
        let no_ledger = palw_class_context_row(two, None, None);
        assert_eq!(no_ledger.source, "unknown", "a node built without a ledger answers from the chain alone");
    }
}
