use crate::{
    consensus::{
        services::{
            ConsensusServices, DbBlockDepthManager, DbDagTraversalManager, DbGhostdagManager, DbParentsManager, DbPruningPointManager,
            DbWindowManager,
        },
        storage::ConsensusStorage,
    },
    constants::BLOCK_VERSION,
    errors::RuleError,
    model::{
        services::{
            reachability::{MTReachabilityService, ReachabilityService},
            relations::MTRelationsService,
        },
        stores::{
            DB,
            acceptance_data::{AcceptanceDataStoreReader, DbAcceptanceDataStore},
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
            block_window_cache::{BlockWindowCacheStore, BlockWindowCacheWriter},
            daa::DbDaaStore,
            depth::{DbDepthStore, DepthStoreReader},
            dns_state::{DbDnsStateStore, DnsStateStoreReader},
            evm::{
                DbEvmCanonicalHeadsStore, DbEvmHeaderStore, DbEvmPayloadStore, DbEvmStateStore, EvmCanonicalHeadsStoreReader,
                EvmHeaderStore, EvmHeaderStoreReader, EvmStateStore, EvmStateStoreReader,
            },
            epoch_accumulator::{DbBlockQualityPoolStore, DbEpochAccumulatorStore, DbReserveBalanceStore},
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            past_pruning_points::DbPastPruningPointsStore,
            pruning::{DbPruningStore, PruningStoreReader},
            pruning_meta::PruningMetaStores,
            pruning_overlay_snapshot::{DbPruningPointOverlaySnapshotStore, PruningPointOverlaySnapshotStoreReader},
            pruning_samples::DbPruningSamplesStore,
            reachability::DbReachabilityStore,
            relations::{DbRelationsStore, RelationsStoreReader},
            rewarded_epochs::{DbRewardedEpochsStore, RewardedEpochKeys, RewardedEpochsStoreReader},
            selected_chain::{DbSelectedChainStore, SelectedChainStore},
            stake_bonds::{DbStakeBondsStore, StakeBondsStoreReader},
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
            tips::{DbTipsStore, TipsStoreReader},
            utxo_diffs::{DbUtxoDiffsStore, UtxoDiffsStoreReader},
            utxo_multisets::{DbUtxoMultisetsStore, UtxoMultisetsStoreReader},
            virtual_state::{LkgVirtualState, VirtualState, VirtualStateStoreReader, VirtualStores},
        },
    },
    params::Params,
    pipeline::{
        ProcessingCounters, deps_manager::VirtualStateProcessingMessage, pruning_processor::processor::PruningProcessingMessage,
        virtual_processor::utxo_validation::UtxoProcessingContext,
    },
    processes::{
        coinbase::CoinbaseManager,
        ghostdag::ordering::SortableBlock,
        transaction_validator::{TransactionValidator, errors::TxResult, tx_validation_in_utxo_context::TxValidationFlags},
        window::WindowManager,
    },
};
use kaspa_consensus_core::{
    BlockHash, BlockHashSet, BlueWorkType, ChainPath,
    acceptance_data::AcceptanceData,
    api::args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    block::{BlockTemplate, EvmClaimStaleKind, MutableBlock, TemplateBuildMode, TemplateTransactionSelector},
    blockstatus::BlockStatus::{StatusDisqualifiedFromChain, StatusUTXOValid},
    coinbase::MinerData,
    config::genesis::GenesisBlock,
    dns_finality::{
        ATTESTATION_MLDSA87_CONTEXT, ActiveBondView, AttestationContribution, BlockEpochContribution, BondMutation, BondStatus,
        CanonicalLaggedEpochAnchor, DnsParams, DnsReorgMode, DnsRolloutStage, StakeBondRecord, StakeScore,
        advance_dns_confirmation, aggregate_epoch_tallies, anchor_cutoff_blue_score, attestations_from_accepted_txs,
        bond_mutations_from_accepted_txs, canonical_lagged_epoch_anchor, check_dns_reorg_rule, compute_stake_score,
        derive_dns_health, effective_bond_status, is_bond_active_at, ready_epoch_from_tip_blue_score, recompute_epoch_tallies,
        reorg_inputs_since_common_ancestor, stake_attestation_message, total_active_stake_by_epoch, BlockOverlayContribution,
        OverlaySnapshot, PruningPointOverlaySnapshot,
    },
    header::Header,
    merkle::calc_hash_merkle_root,
    mining_rules::MiningRules,
    pruning::PruningPointsList,
    tx::{MutableTransaction, Transaction},
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
    },
};
use kaspa_consensus_notify::{
    notification::{
        NewBlockTemplateNotification, Notification, SinkBlueScoreChangedNotification, UtxosChangedNotification,
        VirtualChainChangedNotification, VirtualDaaScoreChangedNotification,
    },
    root::ConsensusNotificationRoot,
};
use kaspa_consensusmanager::SessionLock;
use kaspa_core::{debug, info, time::unix_now, trace, warn};
use kaspa_database::prelude::{StoreError, StoreResultExt, StoreResultUnitExt};
use kaspa_hashes::ZERO_HASH64;
use kaspa_muhash::MuHash;
use kaspa_notify::{events::EventType, notifier::Notify};
use once_cell::unsync::Lazy;

use super::errors::{PruningImportError, PruningImportResult};
use crossbeam_channel::{Receiver as CrossbeamReceiver, Sender as CrossbeamSender};
use itertools::Itertools;
use kaspa_consensus_core::tx::ValidatedTransaction;
use kaspa_txscript::verify_mldsa87_with_context;
use kaspa_utils::binary_heap::BinaryHeapExtensions;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rand::{Rng, seq::SliceRandom};
use rayon::{
    ThreadPool,
    prelude::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator},
};
use rocksdb::WriteBatch;
use std::{
    cmp::min,
    collections::{BTreeMap, BinaryHeap, HashMap, VecDeque},
    ops::Deref,
    sync::{Arc, atomic::Ordering},
};

/// O9 (optimization design v0.1): rolling EVM-lane throughput counters.
/// Recorded only on the `evm` chain-context step, so it is dead on the default
/// (secp-free, non-`evm`) node — silence the dead-code lint there.
#[cfg_attr(not(feature = "evm"), allow(dead_code))]
#[derive(Default)]
pub(super) struct EvmLaneKpi {
    chain_blocks: std::sync::atomic::AtomicU64,
    mergeset_blocks: std::sync::atomic::AtomicU64,
    accepted_gas: std::sync::atomic::AtomicU64,
    // kaspa-pq EVM bridge observability: cumulative deposit-claims APPLIED in
    // accepted chain blocks. Surfaced in the KPI line because accepted-gas
    // utilization rounds to 0.00% even for several successful claims (one claim
    // ≈ 25k gas of the 30M cap ≈ 0.00065%), so "0.00%" must NOT be read as "zero
    // claims succeeded" — this counter is the direct success signal.
    applied_claims: std::sync::atomic::AtomicU64,
}

#[cfg_attr(not(feature = "evm"), allow(dead_code))]
impl EvmLaneKpi {
    /// Record one validated EVM chain block (and the deposit claims it applied);
    /// periodically logs the rolling averages + cumulative applied claims (every
    /// 256 chain blocks).
    pub(super) fn record(&self, mergeset_size: usize, gas_used: u64, claims_applied: usize) {
        use std::sync::atomic::Ordering;
        let n = self.chain_blocks.fetch_add(1, Ordering::Relaxed) + 1;
        let ms = self.mergeset_blocks.fetch_add(mergeset_size as u64, Ordering::Relaxed) + mergeset_size as u64;
        let gas = self.accepted_gas.fetch_add(gas_used, Ordering::Relaxed) + gas_used;
        let claims = self.applied_claims.fetch_add(claims_applied as u64, Ordering::Relaxed) + claims_applied as u64;
        if n.is_multiple_of(256) {
            let cap = kaspa_consensus_core::evm::MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK as f64;
            info!(
                "EVM lane KPI (O9): {} chain blocks, avg mergeset {:.2}, avg accepted-gas utilization {:.2}%, {} deposit-claims applied (cumulative)",
                n,
                ms as f64 / n as f64,
                (gas as f64 / n as f64) / cap * 100.0,
                claims
            );
        }
    }
}

pub struct VirtualStateProcessor {
    // Channels
    receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
    pruning_sender: CrossbeamSender<PruningProcessingMessage>,
    pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    db: Arc<DB>,

    // Config
    pub(super) genesis: GenesisBlock,
    pub(super) max_block_parents: u8,
    pub(super) mergeset_size_limit: u64,
    /// kaspa-pq Phase 3 PoW (ADR-0007): BLAKE2b-512 ∥ SHA3-512 (`algo_id = 3`) activation — sets the
    /// block template's `pow_algo_id` so miners produce the network-correct Layer-1 algorithm.
    pub(super) pow_blake2b_sha3_activation: kaspa_consensus_core::config::params::ForkActivation,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) daa_excluded_store: Arc<DbDaaStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub(super) pruning_point_store: Arc<RwLock<DbPruningStore>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    pub(super) body_tips_store: Arc<RwLock<DbTipsStore>>,
    pub(super) depth_store: Arc<DbDepthStore>,
    pub(super) selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,
    pub(super) pruning_samples_store: Arc<DbPruningSamplesStore>,

    // kaspa-pq Phase 10 (ADR-0009): DNS finality overlay. `dns_params` is the
    // dormancy guard — `None` on every current network, so the bond-population
    // pass below is a single `Option` check and a return.
    pub(super) stake_bonds_store: Arc<RwLock<DbStakeBondsStore>>,
    pub(super) dns_state_store: Arc<RwLock<DbDnsStateStore>>,
    // kaspa-pq ADR-0022: overlay snapshot as-of the pruning point (serve + below-pp window consult).
    pub(super) pruning_overlay_snapshot_store: Arc<RwLock<DbPruningPointOverlaySnapshotStore>>,
    pub(super) dns_params: Option<DnsParams>,

    // kaspa-pq Selected-Parent EVM Lane (ADR-0020, design v0.4). The lazy
    // chain-context EVM step + canonical head pointers. Inert until
    // `evm_activation_daa_score` is finite (`u64::MAX` on every current net).
    pub(super) evm_header_store: Arc<DbEvmHeaderStore>,
    pub(super) evm_state_store: Arc<DbEvmStateStore>,
    #[cfg_attr(not(feature = "evm"), allow(dead_code))] // read by the cfg(evm) chain-context step only
    pub(super) evm_payload_store: Arc<DbEvmPayloadStore>,
    pub(super) evm_heads_store: Arc<RwLock<DbEvmCanonicalHeadsStore>>,
    pub(super) evm_receipts_store: Arc<crate::model::stores::evm::DbEvmReceiptsStore>,
    pub(super) evm_tx_index_store: Arc<crate::model::stores::evm::DbEvmTxIndexStore>,
    pub(super) evm_block_hash_map_store: Arc<crate::model::stores::evm::DbEvmBlockHashMapStore>,
    pub(super) evm_number_store: Arc<crate::model::stores::evm::DbEvmNumberStore>,
    pub(super) evm_activation_daa_score: u64,
    pub(super) evm_gas_pool_v2_activation_daa_score: u64,
    pub(super) evm_f002_withdraw_cap_activation_daa_score: u64,
    pub(super) evm_f003_mldsa_verify_activation_daa_score: u64,
    // O9 (optimization design v0.1): node-local EVM-lane KPIs — chain-block
    // count / mergeset-size sum / accepted-gas sum. The gas supply is
    // 30M × chain-block rate (NOT DAG width), and the adversarial degradation
    // mode is a widening mergeset (design §2/B7) — these counters make that
    // observable. Logged every 256 chain blocks; never consensus-relevant.
    #[cfg_attr(not(feature = "evm"), allow(dead_code))] // recorded only on the cfg(evm) chain-context step
    pub(super) evm_lane_kpi: EvmLaneKpi,

    // Utxo-related stores
    pub(super) utxo_diffs_store: Arc<DbUtxoDiffsStore>,
    // kaspa-pq DNS overlay (ADR-0009 Addendum B §B.3(c)): per-block rewarded
    // `(bond, epoch)` keys for cross-block reward uniqueness.
    pub(super) rewarded_epochs_store: Arc<DbRewardedEpochsStore>,
    // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): the per-epoch accumulator and
    // its per-block validator quality sub-pool input. Inert until
    // `pos_v2_activation_daa_score` (`u64::MAX` today).
    pub(super) epoch_accumulator_store: Arc<DbEpochAccumulatorStore>,
    pub(super) block_quality_pool_store: Arc<DbBlockQualityPoolStore>,
    pub(super) reserve_balance_store: Arc<DbReserveBalanceStore>,
    pub(super) utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
    pub(super) acceptance_data_store: Arc<DbAcceptanceDataStore>,
    pub(super) virtual_stores: Arc<RwLock<VirtualStores>>,
    pub(super) pruning_meta_stores: Arc<RwLock<PruningMetaStores>>,

    /// The "last known good" virtual state. To be used by any logic which does not want to wait
    /// for a possible virtual state write to complete but can rather settle with the last known state
    pub lkg_virtual_state: LkgVirtualState,

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) relations_service: MTRelationsService<DbRelationsStore>,
    pub(super) dag_traversal_manager: DbDagTraversalManager,
    pub(super) window_manager: DbWindowManager,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(super) transaction_validator: TransactionValidator,
    pub(super) pruning_point_manager: DbPruningPointManager,
    pub(super) parents_manager: DbParentsManager,
    pub(super) depth_manager: DbBlockDepthManager,

    // block window caches
    pub(super) block_window_cache_for_difficulty: Arc<BlockWindowCacheStore>,
    pub(super) block_window_cache_for_past_median_time: Arc<BlockWindowCacheStore>,

    // Pruning lock
    pub(super) pruning_lock: SessionLock,

    // Notifier
    notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    counters: Arc<ProcessingCounters>,

    // Mining Rule
    _mining_rules: Arc<MiningRules>,
}

impl VirtualStateProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
        pruning_sender: CrossbeamSender<PruningProcessingMessage>,
        pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,
        thread_pool: Arc<ThreadPool>,
        params: &Params,
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        services: &Arc<ConsensusServices>,
        pruning_lock: SessionLock,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
        mining_rules: Arc<MiningRules>,
    ) -> Self {
        Self {
            receiver,
            pruning_sender,
            pruning_receiver,
            thread_pool,

            genesis: params.genesis.clone(),
            pow_blake2b_sha3_activation: params.pow_blake2b_sha3_activation,
            max_block_parents: params.max_block_parents(),
            mergeset_size_limit: params.mergeset_size_limit(),

            db,
            statuses_store: storage.statuses_store.clone(),
            headers_store: storage.headers_store.clone(),
            ghostdag_store: storage.ghostdag_store.clone(),
            daa_excluded_store: storage.daa_excluded_store.clone(),
            block_transactions_store: storage.block_transactions_store.clone(),
            pruning_point_store: storage.pruning_point_store.clone(),
            past_pruning_points_store: storage.past_pruning_points_store.clone(),
            body_tips_store: storage.body_tips_store.clone(),
            depth_store: storage.depth_store.clone(),
            selected_chain_store: storage.selected_chain_store.clone(),
            pruning_samples_store: storage.pruning_samples_store.clone(),
            stake_bonds_store: storage.stake_bonds_store.clone(),
            dns_state_store: storage.dns_state_store.clone(),
            pruning_overlay_snapshot_store: storage.pruning_overlay_snapshot_store.clone(),
            evm_header_store: storage.evm_header_store.clone(),
            evm_state_store: storage.evm_state_store.clone(),
            evm_payload_store: storage.evm_payload_store.clone(),
            evm_heads_store: storage.evm_heads_store.clone(),
            evm_receipts_store: storage.evm_receipts_store.clone(),
            evm_tx_index_store: storage.evm_tx_index_store.clone(),
            evm_block_hash_map_store: storage.evm_block_hash_map_store.clone(),
            evm_number_store: storage.evm_number_store.clone(),
            evm_activation_daa_score: params.evm_activation_daa_score,
            evm_gas_pool_v2_activation_daa_score: params.evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score: params.evm_f002_withdraw_cap_activation_daa_score,
            evm_f003_mldsa_verify_activation_daa_score: params.evm_f003_mldsa_verify_activation_daa_score,
            evm_lane_kpi: EvmLaneKpi::default(),
            dns_params: params.dns_params.clone(),
            utxo_diffs_store: storage.utxo_diffs_store.clone(),
            rewarded_epochs_store: storage.rewarded_epochs_store.clone(),
            epoch_accumulator_store: storage.epoch_accumulator_store.clone(),
            block_quality_pool_store: storage.block_quality_pool_store.clone(),
            reserve_balance_store: storage.reserve_balance_store.clone(),
            utxo_multisets_store: storage.utxo_multisets_store.clone(),
            acceptance_data_store: storage.acceptance_data_store.clone(),
            virtual_stores: storage.virtual_stores.clone(),
            pruning_meta_stores: storage.pruning_meta_stores.clone(),
            lkg_virtual_state: storage.lkg_virtual_state.clone(),

            block_window_cache_for_difficulty: storage.block_window_cache_for_difficulty.clone(),
            block_window_cache_for_past_median_time: storage.block_window_cache_for_past_median_time.clone(),

            ghostdag_manager: services.ghostdag_manager.clone(),
            reachability_service: services.reachability_service.clone(),
            relations_service: services.relations_service.clone(),
            dag_traversal_manager: services.dag_traversal_manager.clone(),
            window_manager: services.window_manager.clone(),
            coinbase_manager: services.coinbase_manager.clone(),
            transaction_validator: services.transaction_validator.clone(),
            pruning_point_manager: services.pruning_point_manager.clone(),
            parents_manager: services.parents_manager.clone(),
            depth_manager: services.depth_manager.clone(),

            pruning_lock,
            notification_root,
            counters,
            _mining_rules: mining_rules,
        }
    }

    pub fn worker(self: &Arc<Self>) {
        'outer: while let Ok(msg) = self.receiver.recv() {
            if msg.is_exit_message() {
                break;
            }

            // Once a task arrived, collect all pending tasks from the channel.
            // This is done since virtual processing is not a per-block
            // operation, so it benefits from max available info

            let messages: Vec<VirtualStateProcessingMessage> = std::iter::once(msg).chain(self.receiver.try_iter()).collect();
            trace!("virtual processor received {} tasks", messages.len());

            self.resolve_virtual();

            let statuses_read = self.statuses_store.read();
            for msg in messages {
                match msg {
                    VirtualStateProcessingMessage::Exit => break 'outer,
                    VirtualStateProcessingMessage::Process(task, virtual_state_result_transmitter) => {
                        // We don't care if receivers were dropped
                        let _ = virtual_state_result_transmitter.send(Ok(statuses_read.get(task.block().hash()).unwrap()));
                    }
                };
            }
        }

        // Pass the exit signal on to the following processor
        self.pruning_sender.send(PruningProcessingMessage::Exit).unwrap();
    }

    fn resolve_virtual(self: &Arc<Self>) {
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let virtual_read = self.virtual_stores.upgradable_read();
        let prev_state = virtual_read.state.get().unwrap();
        let finality_point = self.virtual_finality_point(&prev_state.ghostdag_data, pruning_point);

        // PRUNE SAFETY: in order to avoid locking the prune lock throughout virtual resolving we make sure
        // to only process blocks in the future of the finality point (F) which are never pruned (since finality depth << pruning depth).
        // This is justified since:
        //      1. Tips which are not in the future of F definitely don't have F on their chain
        //         hence cannot become the next sink (due to finality violation).
        //      2. Such tips cannot be merged by virtual since they are violating the merge depth
        //         bound (merge depth <= finality depth).
        // (both claims are true by induction for any block in their past as well)
        let prune_guard = self.pruning_lock.blocking_read();
        let tips = self
            .body_tips_store
            .read()
            .get()
            .unwrap()
            .read()
            .iter()
            .copied()
            .filter(|&h| self.reachability_service.is_dag_ancestor_of(finality_point, h))
            .collect_vec();
        drop(prune_guard);
        let prev_sink = prev_state.ghostdag_data.selected_parent;
        let mut accumulated_diff = prev_state.utxo_diff.clone().to_reversed();

        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B): the per-block active-bond
        // view, walked in lockstep with `accumulated_diff` so that at each
        // chain-block UTXO verification it equals the bond set as-of that
        // block's selected parent (the deterministic, as-of-block bond
        // resolution the validator-reward coinbase fan-out needs — PR-10.5′-b3).
        // Seeded from the `StakeBonds` store snapshot (= state at `prev_sink`);
        // empty + untouched on networks without the overlay (`dns_params` None).
        // No consumer yet (b2a): `verify_expected_utxo_state` receives it inert.
        let mut accumulated_bond_view = self.initial_active_bond_view();

        let (new_sink, virtual_parent_candidates) = self.sink_search_algorithm(
            &virtual_read,
            &mut accumulated_diff,
            &mut accumulated_bond_view,
            prev_sink,
            tips,
            finality_point,
            pruning_point,
        );
        let (virtual_parents, virtual_ghostdag_data) = self.pick_virtual_parents(new_sink, virtual_parent_candidates, pruning_point);
        assert_eq!(virtual_ghostdag_data.selected_parent, new_sink);

        let sink_multiset = self.utxo_multisets_store.get(new_sink).unwrap();
        let chain_path = self.dag_traversal_manager.calculate_chain_path(prev_sink, new_sink, None);
        let sink_ghostdag_data = Lazy::new(|| self.ghostdag_store.get_data(new_sink).unwrap());
        // Cache the DAA and Median time windows of the sink for future use, as well as prepare for virtual's window calculations
        self.cache_sink_windows(new_sink, prev_sink, &sink_ghostdag_data);

        let new_virtual_state = self
            .calculate_and_commit_virtual_state(
                virtual_read,
                virtual_parents,
                virtual_ghostdag_data,
                sink_multiset,
                &mut accumulated_diff,
                // After `sink_search_algorithm` the walked view equals the bond
                // set as-of the new sink (= the virtual block's selected parent).
                &accumulated_bond_view,
                &chain_path,
            )
            .expect("all possible rule errors are unexpected here");

        let compact_sink_ghostdag_data = if let Some(sink_ghostdag_data) = Lazy::get(&sink_ghostdag_data) {
            // If we had to retrieve the full data, we convert it to compact
            sink_ghostdag_data.to_compact()
        } else {
            // Else we query the compact data directly.
            self.ghostdag_store.get_compact_data(new_sink).unwrap()
        };

        // Update the pruning processor about the virtual state change
        // Empty the channel before sending the new message. If pruning processor is busy, this step makes sure
        // the internal channel does not grow with no need (since we only care about the most recent message)
        let _consume = self.pruning_receiver.try_iter().count();
        self.pruning_sender.send(PruningProcessingMessage::Process { sink_ghostdag_data: compact_sink_ghostdag_data }).unwrap();

        // Emit notifications
        let accumulated_diff = Arc::new(accumulated_diff);
        let virtual_parents = Arc::new(new_virtual_state.parents.clone());
        self.notification_root
            .notify(Notification::NewBlockTemplate(NewBlockTemplateNotification {}))
            .expect("expecting an open unbounded channel");
        self.notification_root
            .notify(Notification::UtxosChanged(UtxosChangedNotification::new(accumulated_diff, virtual_parents)))
            .expect("expecting an open unbounded channel");
        self.notification_root
            .notify(Notification::SinkBlueScoreChanged(SinkBlueScoreChangedNotification::new(compact_sink_ghostdag_data.blue_score)))
            .expect("expecting an open unbounded channel");
        self.notification_root
            .notify(Notification::VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification::new(new_virtual_state.daa_score)))
            .expect("expecting an open unbounded channel");
        if self.notification_root.has_subscription(EventType::VirtualChainChanged) {
            // check for subscriptions before the heavy lifting
            let added_chain_blocks_acceptance_data =
                chain_path.added.iter().copied().map(|added| self.acceptance_data_store.get(added).unwrap()).collect_vec();
            self.notification_root
                .notify(Notification::VirtualChainChanged(VirtualChainChangedNotification::new(
                    chain_path.added.into(),
                    chain_path.removed.into(),
                    Arc::new(added_chain_blocks_acceptance_data),
                )))
                .expect("expecting an open unbounded channel");
        }
    }

    pub(crate) fn virtual_finality_point(&self, virtual_ghostdag_data: &GhostdagData, pruning_point: BlockHash) -> BlockHash {
        let finality_point = self.depth_manager.calc_finality_point(virtual_ghostdag_data, pruning_point);
        if self.reachability_service.is_chain_ancestor_of(pruning_point, finality_point) {
            finality_point
        } else {
            // At the beginning of IBD when virtual finality point might be below the pruning point
            // or disagreeing with the pruning point chain, we take the pruning point itself as the finality point
            pruning_point
        }
    }

    /// Calculates the UTXO state of `to` starting from the state of `from`.
    /// The provided `diff` is assumed to initially hold the UTXO diff of `from` from virtual.
    /// The function returns the top-most UTXO-valid block on `chain(to)` which is ideally
    /// `to` itself (with the exception of returning `from` if `to` is already known to be UTXO disqualified).
    /// When returning it is guaranteed that `diff` holds the diff of the returned block from virtual
    fn calculate_utxo_state_relatively(
        &self,
        stores: &VirtualStores,
        diff: &mut UtxoDiff,
        bond_view: &mut ActiveBondView,
        from: BlockHash,
        to: BlockHash,
    ) -> BlockHash {
        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.1): walk the active-bond
        // view in lockstep with `diff` so it always equals the bond set as-of
        // the block whose UTXO state `diff` represents. No-op on networks
        // without the overlay. No consumer yet (b2a) — the view is passed to
        // `verify_expected_utxo_state` inert.
        let track_bonds = self.dns_params.is_some();

        // Avoid reorging if disqualified status is already known
        if self.statuses_store.read().get(to).unwrap() == StatusDisqualifiedFromChain {
            return from;
        }

        let mut split_point: Option<BlockHash> = None;

        // Walk down to the reorg split point
        for current in self.reachability_service.default_backward_chain_iterator(from) {
            if self.reachability_service.is_chain_ancestor_of(current, to) {
                split_point = Some(current);
                break;
            }

            let mergeset_diff = self.utxo_diffs_store.get(current).unwrap();
            // Apply the diff in reverse
            diff.with_diff_in_place(&mergeset_diff.as_reversed()).unwrap();
            if track_bonds {
                // Mirror the reverse on the bond view. `current` is leaving the
                // selected chain, so its acceptance data is committed.
                bond_view.revert(&self.dns_bond_mutations_for_chain_block(current));
            }
        }

        let split_point = split_point.expect("chain iterator was expected to reach the reorg split point");
        debug!("VIRTUAL PROCESSOR, found split point: {split_point}");

        // O12 (IBD catch-up): when the walk ahead contains a long run of
        // pending chain blocks, pre-execute their EVM acceptance on a pipeline
        // worker overlapped with this thread's serial UTXO validation. Inert
        // when the lane is inactive, on short walks (steady state: 1 block),
        // and on non-evm builds. Commits stay HERE, in canonical order.
        let evm_pipeline = self.maybe_spawn_evm_pipeline(split_point, to);

        // A variable holding the most recent UTXO-valid block on `chain(to)` (note that it's maintained such
        // that 'diff' is always its UTXO diff from virtual)
        let mut diff_point = split_point;

        // Walk back up to the new virtual selected parent candidate
        let mut chain_block_counter = 0;
        let mut chain_disqualified_counter = 0;
        for (selected_parent, current) in self.reachability_service.forward_chain_iterator(split_point, to, true).tuple_windows() {
            if selected_parent != diff_point {
                // This indicates that the selected parent is disqualified, propagate up and continue
                let statuses_guard = self.statuses_store.upgradable_read();
                if statuses_guard.get(current).unwrap() != StatusDisqualifiedFromChain {
                    RwLockUpgradableReadGuard::upgrade(statuses_guard).set(current, StatusDisqualifiedFromChain).unwrap();
                    chain_disqualified_counter += 1;
                }
                continue;
            }

            match self.utxo_diffs_store.get(current) {
                Ok(mergeset_diff) => {
                    diff.with_diff_in_place(mergeset_diff.deref()).unwrap();
                    diff_point = current;
                    if track_bonds {
                        // `current` is an already-validated chain block joining
                        // the diff; its acceptance data is committed.
                        bond_view.apply(&self.dns_bond_mutations_for_chain_block(current));
                    }
                }
                Err(StoreError::KeyNotFound(_)) => {
                    if self.statuses_store.read().get(current).unwrap() == StatusDisqualifiedFromChain {
                        // Current block is already known to be disqualified
                        continue;
                    }

                    let header = self.headers_store.get_header(current).unwrap();
                    let mergeset_data = self.ghostdag_store.get_data(current).unwrap();
                    let pov_daa_score = header.daa_score;

                    let selected_parent_multiset_hash = self.utxo_multisets_store.get(selected_parent).unwrap();
                    let selected_parent_utxo_view = (&stores.utxo_set).compose(&*diff);

                    let mut ctx = UtxoProcessingContext::new(mergeset_data.into(), selected_parent_multiset_hash);

                    // `bond_view` currently equals the bond set as-of `selected_parent`
                    // (the verify point's selected-parent view — Addendum B §B.3),
                    // so it is the same view both `calculate_utxo_state` (slashing
                    // side-effect, PR-16.4-b2) and `verify_expected_utxo_state` read.
                    self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, &*bond_view, pov_daa_score);

                    // kaspa-pq EVM Lane v0.4 (§2.3/§9): the lazy chain-context
                    // EVM step — the FIRST time a block becomes a selected-chain
                    // candidate (this KeyNotFound arm), validate its deposit
                    // claims, execute its mergeset acceptance, verify
                    // `evm_commitment_root`, and fold the bridge's UTXO
                    // side-effects (consumed locks + synthetic withdrawal
                    // outputs) into ctx BEFORE `verify_expected_utxo_state`, so
                    // the header's `utxo_commitment` covers them. A fault
                    // disqualifies the block from the chain exactly like a UTXO
                    // fault (no poison; the block stays in the DAG). A single
                    // u64 compare while the lane is inert.
                    let evm_staged = match self.evm_chain_context_step(
                        current,
                        selected_parent,
                        &header,
                        &mut ctx,
                        &selected_parent_utxo_view,
                        evm_pipeline.as_ref(),
                    ) {
                        Ok(staged) => staged,
                        Err(evm_error) => {
                            info!("Block {} is disqualified from virtual chain (EVM): {}", current, evm_error);
                            self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                            chain_disqualified_counter += 1;
                            continue;
                        }
                    };

                    let res = self.verify_expected_utxo_state(&mut ctx, &selected_parent_utxo_view, &*bond_view, &header);

                    if let Err(rule_error) = res {
                        info!("Block {} is disqualified from virtual chain: {}", current, rule_error);
                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                        chain_disqualified_counter += 1;
                    } else {
                        debug!("VIRTUAL PROCESSOR, UTXO validated for {current}");

                        // Accumulate the diff
                        diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();
                        // Update the diff point
                        diff_point = current;
                        if track_bonds {
                            // Advance the bond view by THIS block's mutations,
                            // derived from the in-memory acceptance data (its
                            // store entry is written by the commit just below).
                            let bond_muts = self.dns_bond_mutations_from_acceptance(&ctx.mergeset_acceptance_data, pov_daa_score);
                            bond_view.apply(&bond_muts);
                        }
                        // Commit UTXO data for current chain block
                        self.commit_utxo_state(
                            current,
                            ctx.mergeset_diff,
                            ctx.multiset_hash,
                            ctx.mergeset_acceptance_data,
                            ctx.pruning_sample_from_pov.expect("verified"),
                            ctx.validator_rewarded_keys,
                            ctx.validator_quality_subpool,
                            ctx.reserve_balance_after,
                            evm_staged,
                        );
                        // Count the number of UTXO-processed chain blocks
                        chain_block_counter += 1;
                    }
                }
                Err(err) => panic!("unexpected error {err}"),
            }
        }
        // Report counters
        self.counters.chain_block_counts.fetch_add(chain_block_counter, Ordering::Relaxed);
        if chain_disqualified_counter > 0 {
            self.counters.chain_disqualified_counts.fetch_add(chain_disqualified_counter, Ordering::Relaxed);
        }

        diff_point
    }

    /// kaspa-pq EVM Lane v0.4 (§2.3): the lazy chain-context EVM step for one
    /// selected-chain candidate. Gated on `evm_activation_daa_score` (a single
    /// u64 compare on every current network); no-replay and the commitment
    /// check live in `processes::evm::evm_validate`. `Err` = the block is
    /// disqualified from the chain (commitment fault), mirroring a UTXO fault.
    #[cfg(feature = "evm")]
    fn evm_chain_context_step<V: UtxoView>(
        &self,
        current: BlockHash,
        selected_parent: BlockHash,
        header: &Header,
        ctx: &mut UtxoProcessingContext<'_>,
        selected_parent_utxo_view: &V,
        pipeline: Option<&crate::processes::evm::EvmPipeline>,
    ) -> Result<Option<crate::processes::evm::EvmStaged>, String> {
        use crate::model::stores::evm::EvmPayloadStoreReader;
        use crate::processes::evm::{apply_evm_bridge_effects, evm_validate, validate_evm_deposit_claims, EvmValidateError};
        if header.daa_score < self.evm_activation_daa_score {
            return Ok(None);
        }
        // The §4.3 version rule admits only v2+ headers at/after activation.
        debug_assert!(header.version >= kaspa_consensus_core::constants::EVM_HEADER_VERSION);
        // B's own payload (system_ops + the accepting coinbase); absent ⇒ empty
        // (only non-empty payloads are persisted at body commit).
        let own_payload = match self.evm_payload_store.get(current) {
            Ok(p) => p,
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => Default::default(),
            Err(e) => return Err(format!("evm payload store: {e}")),
        };
        // §9.2: deposit claims are validated against the CLAIM VIEW = the
        // selected-parent UTXO set composed with the mergeset diff so far (a
        // lock spent by a mergeset tx is not claimable; a same-block lock is
        // not visible). Any violation is an accepting-producer fault.
        let consumed_locks = {
            let claim_view = selected_parent_utxo_view.compose(&ctx.mergeset_diff);
            validate_evm_deposit_claims(&own_payload, &claim_view, header.daa_score)?
        };
        // O12: a pipelined run pre-executed this block's acceptance on the
        // worker (same pure function, same inputs — see EvmPipeline). Consume
        // its result; fall back to inline execution when the pipeline ended.
        let pipelined = pipeline.and_then(|p| p.recv(current));
        let staged = match pipelined {
            Some(Ok(staged)) => Some(staged),
            Some(Err(msg)) => return Err(msg),
            None => {
                // AcceptedEvmTxs(B) source: the consensus-ordered mergeset (selected
                // parent first, then ascending blue work — §3.1 canonical order).
                let sorted_mergeset: Vec<BlockHash> =
                    ctx.ghostdag_data.consensus_ordered_mergeset(self.ghostdag_store.as_ref()).collect();
                evm_validate(
                    &self.evm_header_store,
                    &self.evm_state_store,
                    &self.evm_payload_store,
                    current,
                    selected_parent,
                    &sorted_mergeset,
                    header,
                    &own_payload,
                    self.evm_gas_pool_v2_activation_daa_score,
                    self.evm_f002_withdraw_cap_activation_daa_score,
                    self.evm_f003_mldsa_verify_activation_daa_score,
                )
                .map_err(|e| match e {
                    EvmValidateError::CommitmentMismatch { .. } => {
                        "evm_commitment_root mismatch (mergeset acceptance re-execution)".to_string()
                    }
                    EvmValidateError::Exec(e) => format!("evm execution: {e}"),
                    EvmValidateError::Store(e) => format!("evm store: {e}"),
                })?
            }
        };
        let Some(staged) = staged else {
            // The EVM rows commit in the SAME batch as the UTXO diff, so a
            // present result with an absent diff (this KeyNotFound arm) is
            // store corruption — never a reachable consensus state.
            panic!("EVM result for {current} exists but its UTXO diff does not — corrupt store");
        };
        // §9: fold the bridge's UTXO side-effects into THIS block's diff +
        // multiset (before verify_expected_utxo_state reads them).
        apply_evm_bridge_effects(
            &mut ctx.mergeset_diff,
            &mut ctx.multiset_hash,
            header.daa_score,
            &consumed_locks,
            &staged.result.withdrawals,
        )?;
        // kaspa-pq EVM bridge observability (P0-4): a deposit lock that reaches
        // this point is being APPLIED into this accepted chain block's committed
        // UTXO diff (consumed). Log each so a successful claim is directly visible
        // — the accepted-gas KPI rounds to 0.00% even for several real claims.
        for (outpoint, entry) in &consumed_locks {
            info!(
                "[evm-claim-applied] accepting_block={current} deposit_outpoint={outpoint} amount_sompi={} pov_daa={}",
                entry.amount, header.daa_score
            );
        }
        // O9: chain-rate / mergeset / gas-utilization observability + applied-claim count.
        self.evm_lane_kpi.record(ctx.ghostdag_data.mergeset_size(), staged.result.header.gas_used, consumed_locks.len());
        Ok(Some(staged))
    }

    /// Non-`evm` builds cannot validate the lane. On every default network the
    /// lane is `u64::MAX`-inert so this is unreachable; on an evm-ACTIVE net a
    /// non-evm binary must refuse to follow a chain it cannot validate rather
    /// than silently fork.
    #[cfg(not(feature = "evm"))]
    fn evm_chain_context_step<V: UtxoView>(
        &self,
        _current: BlockHash,
        _selected_parent: BlockHash,
        header: &Header,
        _ctx: &mut UtxoProcessingContext<'_>,
        _selected_parent_utxo_view: &V,
        _pipeline: Option<&crate::processes::evm::EvmPipeline>,
    ) -> Result<Option<crate::processes::evm::EvmStaged>, String> {
        if header.daa_score >= self.evm_activation_daa_score {
            panic!(
                "the EVM lane is active at DAA {} but this kaspad was built without the `evm` feature — refusing to follow a chain it cannot validate (rebuild with --features evm)",
                header.daa_score
            );
        }
        Ok(None)
    }

    /// O12: spawn the EVM pipeline worker for the upcoming forward walk when it
    /// contains a long run of pending EVM-active chain blocks (IBD catch-up).
    /// Steady-state walks (a handful of blocks) skip the pipeline — the thread
    /// + channel overhead outweighs overlapping a single block.
    #[cfg(feature = "evm")]
    fn maybe_spawn_evm_pipeline(&self, split_point: BlockHash, to: BlockHash) -> Option<crate::processes::evm::EvmPipeline> {
        use crate::processes::evm::{EvmPipeline, EvmPipelineItem};
        const MIN_PIPELINE_RUN: usize = 8;
        if self.evm_activation_daa_score == u64::MAX {
            return None;
        }
        let statuses = self.statuses_store.read();
        let mut pending: Vec<EvmPipelineItem> = Vec::new();
        let mut prev_pending: Option<BlockHash> = None;
        for (selected_parent, current) in self.reachability_service.forward_chain_iterator(split_point, to, true).tuple_windows() {
            // Mirror the walk's KeyNotFound arm: only blocks without a committed
            // UTXO diff and not already disqualified will be validated.
            if self.utxo_diffs_store.get(current).is_ok() {
                continue;
            }
            if statuses.get(current).unwrap() == StatusDisqualifiedFromChain {
                continue;
            }
            if self.headers_store.get_daa_score(current).unwrap() < self.evm_activation_daa_score {
                continue; // pre-activation block: the step is inert for it
            }
            let chain_from_prev = prev_pending == Some(selected_parent);
            pending.push(EvmPipelineItem { block: current, selected_parent, chain_from_prev });
            prev_pending = Some(current);
        }
        drop(statuses);
        if pending.len() < MIN_PIPELINE_RUN {
            return None;
        }
        Some(EvmPipeline::spawn(
            self.evm_header_store.clone(),
            self.evm_state_store.clone(),
            self.evm_payload_store.clone(),
            self.headers_store.clone(),
            self.ghostdag_store.clone(),
            pending,
            self.evm_gas_pool_v2_activation_daa_score,
            self.evm_f002_withdraw_cap_activation_daa_score,
            self.evm_f003_mldsa_verify_activation_daa_score,
        ))
    }

    /// Non-`evm` builds never pipeline (the step itself is a panic-guard there).
    #[cfg(not(feature = "evm"))]
    fn maybe_spawn_evm_pipeline(&self, _split_point: BlockHash, _to: BlockHash) -> Option<crate::processes::evm::EvmPipeline> {
        None
    }

    /// kaspa-pq EVM Lane v0.4 (§10 / invariant I3): a virtual change only moves
    /// the canonical EVM head POINTERS — never executes. Pre-§16 (RPC) policy:
    /// `latest` = the new sink; `safe` tracks `latest`; `finalized` tracks the
    /// pruning point once it carries an EVM result (consensus-final), else the
    /// previous finalized. The blue-work-depth `safe` + DNS-confirmed-anchor
    /// `finalized` selection lands with the RPC phase that first exposes the
    /// tags. Inert (one u64 compare) on every current network.
    fn update_evm_canonical_heads(&self, batch: &mut WriteBatch, sink: BlockHash) {
        use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader};
        if self.evm_activation_daa_score == u64::MAX {
            return;
        }
        // The sink carries an EVM result iff the lane is live for it (it may
        // predate activation right after the fork).
        if !self.evm_header_store.has(sink).unwrap_or(false) {
            return;
        }
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let prev_finalized = self.evm_heads_store.read().get().ok().map(|h| h.finalized);
        let finalized = if self.evm_header_store.has(pruning_point).unwrap_or(false) {
            pruning_point
        } else {
            prev_finalized.unwrap_or(sink)
        };
        let heads = kaspa_consensus_core::evm::CanonicalEvmHeads { latest: sink, safe: sink, finalized };
        self.evm_heads_store.write().set_batch(batch, heads).unwrap();
    }

    /// kaspa-pq EVM Lane v0.4 (§15): producer-side EVM fields for a template
    /// built from the current virtual state. Runs the SAME acceptance-execution
    /// core the verifier uses, so a block mined from this template reproduces
    /// `evm_commitment_root` byte-for-byte. The own payload is empty until the
    /// EVM mempool lands (§16). NOTE: the commitment derives from the header's
    /// timestamp — a miner must not mutate the template timestamp (refreshing
    /// the template re-derives the commitment).
    #[cfg(feature = "evm")]
    fn evm_template_fields(
        &self,
        header: Header,
        virtual_state: &VirtualState,
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
        // kaspa-pq narrow P0-1: deposit claims already validated + their lock
        // entries materialized against the template's virtual generation (no
        // re-read of a possibly-advanced view here).
        prepared_claims: crate::processes::evm::PreparedDepositClaims,
    ) -> Result<
        (Header, kaspa_consensus_core::evm::EvmExecutionPayload, Vec<(kaspa_consensus_core::tx::TransactionOutpoint, EvmClaimStaleKind)>),
        RuleError,
    > {
        use crate::processes::evm::evm_execute_acceptance;
        if header.daa_score < self.evm_activation_daa_score {
            return Ok((header, Default::default(), vec![]));
        }
        // narrow P0-1: split the deposit-claim snapshot prepared against the
        // template's virtual generation — `accepted` claims go into the payload,
        // their `consumed_locks` fold into the commitment, the `stale` set flows
        // back to the mining manager.
        let crate::processes::evm::PreparedDepositClaims { accepted: accepted_claims, consumed_locks, stale: stale_claims } =
            prepared_claims;
        // §15 step 6: assemble the own payload from the mempool candidates.
        // Defense-in-depth re-admission (the body class-1 rule): an inadmissible
        // tx here would make our OWN block payload-block-invalid, so hard-filter
        // rather than trust the pool; independently re-enforce the byte cap.
        // The candidates execute in a LATER accepting chain block, never here.
        let own_payload = {
            use kaspa_consensus_core::evm::{EvmExecutionPayload, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK};
            let mut payload = EvmExecutionPayload::default();
            let base = payload.payload_bytes().len();
            let mut budget = MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK.saturating_sub(base);
            for raw in evm_template_data.transactions {
                if 4 + raw.len() > budget {
                    continue;
                }
                match crate::processes::evm::admit_evm_payload_txs(&EvmExecutionPayload {
                    transactions: vec![raw.clone()],
                    ..Default::default()
                }) {
                    Ok(()) => {
                        budget -= 4 + raw.len();
                        payload.transactions.push(raw);
                    }
                    Err((_, reason)) => {
                        warn!("EVM template: dropping inadmissible mempool candidate ({reason})");
                    }
                }
            }
            // §9.2 (narrow P0-1): own-payload deposit claims. These EXECUTE in the
            // accepting chain block, so an invalid claim would make our block invalid.
            // The claims were ALREADY validated, and their consumed lock entries
            // materialized, by `prepare_deposit_claims` against the SAME virtual
            // generation this template's selected parent is taken from — NOT a
            // re-read of a possibly-advanced view here (that second read was the
            // mixed-generation TOCTOU that could self-disqualify the block or wrongly
            // drop a still-valid claim). The claim view for a block B extending the
            // virtual tip is `selected_parent(B)_view ∘ B.mergeset_diff`, which for a
            // fresh template IS the captured virtual UTXO set — exactly what the
            // acceptance path re-checks. Emit the accepted claims; the consumed locks
            // fold into the commitment below; the tagged stale set flows back to the
            // mining manager (`Absent` ⇒ retain + retry, `Invalid` ⇒ evict).
            for claim in accepted_claims {
                payload.system_ops.push(kaspa_consensus_core::evm::EvmSystemOp::DepositClaim(claim));
            }
            // audit #3: the tx loop above budgets ONLY the txs against the byte
            // cap; the deposit-claim system ops are appended afterwards and each
            // is ~105 bytes, so a near-full tx payload + ≥1 claim can exceed
            // MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK — which body validation rejects,
            // making the node's OWN template invalid. Claims must execute (they
            // are this block's bridge credits), so keep every selected claim and
            // drop trailing (lowest-priority) txs until the WHOLE payload fits.
            while !payload.transactions.is_empty() && payload.payload_bytes().len() > MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK {
                payload.transactions.pop();
            }
            // §8.2: the declared coinbase claims this payload's priority fees —
            // meaningful only when the payload actually carries content (and
            // keeping it zero otherwise preserves the empty payload / empty
            // store-row form). A claim-only payload also declares the coinbase
            // (the claim tip routes to it, §9.2).
            if !payload.transactions.is_empty() || !payload.system_ops.is_empty() {
                payload.evm_coinbase = evm_template_data.evm_coinbase;
            }
            payload
        };
        let sorted_mergeset: Vec<BlockHash> =
            virtual_state.ghostdag_data.consensus_ordered_mergeset(self.ghostdag_store.as_ref()).collect();
        let (result, _snapshot, _candidate_meta) = evm_execute_acceptance(
            &self.evm_header_store,
            &self.evm_state_store,
            &self.evm_payload_store,
            virtual_state.ghostdag_data.selected_parent,
            &sorted_mergeset,
            &header,
            &own_payload,
            self.evm_gas_pool_v2_activation_daa_score,
            self.evm_f002_withdraw_cap_activation_daa_score,
            self.evm_f003_mldsa_verify_activation_daa_score,
        )
        // audit R2-#4: a producer-side acceptance failure (e.g. a local EVM
        // store-integrity error) is a template-build failure, not a panic.
        .map_err(|e| RuleError::EvmTemplateExecutionFailed(format!("{e:?}")))?;
        let mut header = header.with_evm_payload_hash(own_payload.payload_hash()).with_evm_commitment(result.header.commitment_root());
        // §9: the validator folds the bridge's UTXO side-effects (consumed
        // deposit locks + materialized withdrawals) into THIS block's diff and
        // checks them against `header.utxo_commitment` — so the PRODUCER must
        // fold the identical effects into the template's commitment (the
        // template inherited the virtual multiset, which has none of them).
        // Found live: the first claim-bearing template self-disqualified.
        if !consumed_locks.is_empty() || !result.withdrawals.is_empty() {
            let mut multiset = virtual_state.multiset.clone();
            let mut scratch_diff = kaspa_consensus_core::utxo::utxo_diff::UtxoDiff::default();
            crate::processes::evm::apply_evm_bridge_effects(
                &mut scratch_diff,
                &mut multiset,
                header.daa_score,
                &consumed_locks,
                &result.withdrawals,
            )
            .expect("template bridge effects mirror validation on already-validated inputs");
            header.utxo_commitment = multiset.finalize();
            header.finalize();
        }
        Ok((header, own_payload, stale_claims))
    }

    /// Non-`evm` builds cannot produce evm-active templates (same refusal as
    /// the validation seam); unreachable on every default network.
    #[cfg(not(feature = "evm"))]
    fn evm_template_fields(
        &self,
        header: Header,
        _virtual_state: &VirtualState,
        _evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
        _prepared_claims: crate::processes::evm::PreparedDepositClaims,
    ) -> Result<
        (Header, kaspa_consensus_core::evm::EvmExecutionPayload, Vec<(kaspa_consensus_core::tx::TransactionOutpoint, EvmClaimStaleKind)>),
        RuleError,
    > {
        if header.daa_score >= self.evm_activation_daa_score {
            panic!(
                "the EVM lane is active at DAA {} but this kaspad was built without the `evm` feature — cannot build a valid template (rebuild with --features evm)",
                header.daa_score
            );
        }
        Ok((header, Default::default(), vec![]))
    }

    fn commit_utxo_state(
        &self,
        current: BlockHash,
        mergeset_diff: UtxoDiff,
        multiset: MuHash,
        acceptance_data: AcceptanceData,
        pruning_sample_from_pov: BlockHash,
        // kaspa-pq (ADR-0009 Addendum B §B.3(c)): the `(bond, epoch)` keys this
        // block rewarded. Persisted only when non-empty — empty on every block
        // of every current network (the overlay is dormant), so no rows are
        // written there.
        rewarded_keys: RewardedEpochKeys,
        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): this block's validator quality
        // sub-pool, the per-epoch accumulator's recompute input. Non-zero (and
        // therefore persisted) only past `pos_v2_activation_daa_score` (`u64::MAX`
        // today), so no row is written on any current network.
        quality_subpool: u64,
        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): this block's cumulative reserve balance.
        // Persisted only when non-zero (the 0 default is never stored), so no row on any current
        // network. Children read it as their `parent_balance` for the reserve drip.
        reserve_balance: u64,
        // kaspa-pq EVM Lane v0.4 (§2.3): the validated EVM rows staged by
        // `evm_chain_context_step` — committed in THIS batch so the EVM result
        // and the block's UTXO diff are atomic. `None` on every current
        // network (lane inert) and on non-evm builds.
        evm_staged: Option<crate::processes::evm::EvmStaged>,
    ) {
        let mut batch = WriteBatch::default();
        if let Some(staged) = evm_staged {
            self.evm_header_store.insert_batch(&mut batch, current, staged.result.header.clone()).unwrap();
            // §16: receipts + tx-lookup index rows (store/RPC data only) commit
            // in the SAME batch — atomic with the result and the UTXO diff.
            crate::processes::evm::stage_evm_index_rows(
                &self.evm_receipts_store,
                &self.evm_tx_index_store,
                &mut batch,
                current,
                &staged,
            )
            .unwrap();
            self.evm_state_store.insert_batch(&mut batch, current, staged.snapshot).unwrap();
            // §16 eth-rpc: map the 32-byte eth block id (first 32 bytes of the
            // 64-byte L1 hash — the truncation `eth_getTransactionReceipt`
            // already exposes as `blockHash`) → this L1 block, so
            // `eth_getBlockByHash` can reverse a client-held 32-byte hash. Upsert
            // (a given L1 block's first-32 is stable). RPC index only.
            let mut rpc_block_id = [0u8; 32];
            rpc_block_id.copy_from_slice(&current.as_bytes()[..32]);
            self.evm_block_hash_map_store.write_batch(&mut batch, kaspa_hashes::EvmH256::from_bytes(rpc_block_id), current).unwrap();
            // §16 eth-rpc: evm_number → this L1 block (upsert; the reader re-validates
            // canonicality so a reorg-orphaned number reads as absent).
            self.evm_number_store.write_batch(&mut batch, staged.result.header.evm_number, current).unwrap();
        }
        self.utxo_diffs_store.insert_batch(&mut batch, current, Arc::new(mergeset_diff)).unwrap();
        self.utxo_multisets_store.insert_batch(&mut batch, current, multiset).unwrap();
        self.acceptance_data_store.insert_batch(&mut batch, current, Arc::new(acceptance_data)).unwrap();
        if !rewarded_keys.is_empty() {
            self.rewarded_epochs_store.insert_batch(&mut batch, current, Arc::new(rewarded_keys)).unwrap();
        }
        if quality_subpool > 0 {
            self.block_quality_pool_store.insert_batch(&mut batch, current, quality_subpool).unwrap();
        }
        if reserve_balance > 0 {
            self.reserve_balance_store.insert_batch(&mut batch, current, reserve_balance).unwrap();
        }
        // Note we call idempotent since this field can be populated during IBD with headers proof
        self.pruning_samples_store.insert_batch(&mut batch, current, pruning_sample_from_pov).idempotent().unwrap();
        let write_guard = self.statuses_store.set_batch(&mut batch, current, StatusUTXOValid).unwrap();
        self.db.write(batch).unwrap();
        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(write_guard);
    }

    fn calculate_and_commit_virtual_state(
        &self,
        virtual_read: RwLockUpgradableReadGuard<'_, VirtualStores>,
        virtual_parents: Vec<BlockHash>,
        virtual_ghostdag_data: GhostdagData,
        selected_parent_multiset: MuHash,
        accumulated_diff: &mut UtxoDiff,
        // kaspa-pq Phase 10/11 (ADR-0016 §D.4): the bond set as-of the virtual
        // selected parent, walked in lockstep with `accumulated_diff`. Forwarded
        // to `calculate_virtual_state`/`calculate_utxo_state` for the slashing
        // side-effect; inert until PR-16.4-b2 consumes it.
        selected_parent_bond_view: &ActiveBondView,
        chain_path: &ChainPath,
    ) -> Result<Arc<VirtualState>, RuleError> {
        let new_virtual_state = self.calculate_virtual_state(
            &virtual_read,
            virtual_parents,
            virtual_ghostdag_data,
            selected_parent_multiset,
            accumulated_diff,
            selected_parent_bond_view,
        )?;
        self.commit_virtual_state(virtual_read, new_virtual_state.clone(), accumulated_diff, chain_path);
        Ok(new_virtual_state)
    }

    pub(super) fn calculate_virtual_state(
        &self,
        virtual_stores: &VirtualStores,
        virtual_parents: Vec<BlockHash>,
        virtual_ghostdag_data: GhostdagData,
        selected_parent_multiset: MuHash,
        accumulated_diff: &mut UtxoDiff,
        // kaspa-pq Phase 10/11 (ADR-0016 §D.4): the bond set as-of the virtual
        // selected parent (= the new sink). Forwarded to `calculate_utxo_state`
        // for the slashing side-effect; inert until PR-16.4-b2 consumes it.
        selected_parent_bond_view: &ActiveBondView,
    ) -> Result<Arc<VirtualState>, RuleError> {
        let selected_parent_utxo_view = (&virtual_stores.utxo_set).compose(&*accumulated_diff);
        let mut ctx = UtxoProcessingContext::new((&virtual_ghostdag_data).into(), selected_parent_multiset);

        // Calc virtual DAA score, difficulty bits and past median time
        let virtual_daa_window = self.window_manager.block_daa_window(&virtual_ghostdag_data)?;
        let virtual_bits = self.window_manager.calculate_difficulty_bits(&virtual_ghostdag_data, &virtual_daa_window);
        let virtual_past_median_time = self.window_manager.calc_past_median_time(&virtual_ghostdag_data)?.0;

        // Calc virtual UTXO state relative to selected parent
        self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, selected_parent_bond_view, virtual_daa_window.daa_score);

        // Update the accumulated diff
        accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();

        // Build the new virtual state
        Ok(Arc::new(VirtualState::new(
            virtual_parents,
            virtual_daa_window.daa_score,
            virtual_bits,
            virtual_past_median_time,
            ctx.multiset_hash,
            ctx.mergeset_diff,
            ctx.accepted_tx_ids,
            ctx.mergeset_rewards,
            virtual_daa_window.mergeset_non_daa,
            virtual_ghostdag_data,
        )))
    }

    fn commit_virtual_state(
        &self,
        virtual_read: RwLockUpgradableReadGuard<'_, VirtualStores>,
        new_virtual_state: Arc<VirtualState>,
        accumulated_diff: &UtxoDiff,
        chain_path: &ChainPath,
    ) {
        let mut batch = WriteBatch::default();
        let mut virtual_write = RwLockUpgradableReadGuard::upgrade(virtual_read);
        let mut selected_chain_write = self.selected_chain_store.write();

        // Apply the accumulated diff to the virtual UTXO set
        virtual_write.utxo_set.write_diff_batch(&mut batch, accumulated_diff).unwrap();

        // Update virtual state (capture the new sink first — `set_batch` moves the Arc).
        let dns_sink = new_virtual_state.ghostdag_data.selected_parent;
        virtual_write.state.set_batch(&mut batch, new_virtual_state).unwrap();

        // Update the virtual selected chain
        selected_chain_write.apply_changes(&mut batch, chain_path).unwrap();

        // kaspa-pq Phase 10 (ADR-0009 A.4): stage the DNS stake-bond set
        // changes into the same batch so they commit atomically with the
        // virtual state. Inert unless the overlay is configured.
        self.stage_dns_bond_mutations(&mut batch, chain_path);

        // kaspa-pq Phase 10 (ADR-0009 A.5): recompute the DNS StakeScore over
        // the bounded recent epoch window and stage the updated DnsState into
        // the same batch. Inert unless the overlay is configured.
        self.update_dns_state(&mut batch, dns_sink);

        // kaspa-pq EVM Lane v0.4 (§10 / invariant I3): a virtual change only
        // MOVES the canonical EVM head pointers — no execution happens here.
        self.update_evm_canonical_heads(&mut batch, dns_sink);

        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): recompute the per-epoch
        // accumulator over the bounded selected-chain window ending at the new
        // sink and stage it into the same batch. Inert below the v2 fence
        // (`pos_v2_activation_daa_score`, `u64::MAX` today) — returns after a
        // single header read on every current network.
        self.update_epoch_accumulator(&mut batch, dns_sink);

        // Flush the batch changes
        self.db.write(batch).unwrap();

        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(virtual_write);
        drop(selected_chain_write);
    }

    /// kaspa-pq Phase 10 (ADR-0009 Addendum A.4): stage the `StakeBonds`-store
    /// mutations implied by this selected-chain change into `batch`, so they
    /// commit atomically with the virtual state. **Inert** unless the DNS
    /// overlay is configured (`dns_params.is_some()`) — on every current
    /// network this is a single `Option` check and a return.
    ///
    /// Mirrors the UTXO reorg model: blocks leaving the selected chain
    /// (`chain_path.removed`) are reverted, most-recent first, **before**
    /// blocks joining it (`chain_path.added`) are applied. Within a block,
    /// `Insert` reverts by delete and `Slash` by clearing `slashed_at`; a
    /// `Slash` revert whose bond record is already gone (its `Insert` was
    /// reverted in the same range) is skipped gracefully. Acceptance data is
    /// retained on reorg (only pruning deletes it), so removed blocks can be
    /// re-derived deterministically.
    fn stage_dns_bond_mutations(&self, batch: &mut WriteBatch, chain_path: &ChainPath) {
        if self.dns_params.is_none() {
            return;
        }
        let mut store = self.stake_bonds_store.write();

        // Revert blocks that left the selected chain (most-recent first).
        for removed in chain_path.removed.iter().rev().copied() {
            for mutation in self.dns_bond_mutations_for_chain_block(removed).into_iter().rev() {
                match mutation {
                    BondMutation::Insert(outpoint, _) => {
                        store.delete_batch(batch, outpoint).unwrap();
                    }
                    BondMutation::Slash(outpoint, _) => {
                        if let Ok(record) = store.get(&outpoint) {
                            let mut record = (*record).clone();
                            record.slashed_at_daa_score = None;
                            record.status = BondStatus::Active;
                            store.insert_batch(batch, outpoint, Arc::new(record)).unwrap();
                        }
                    }
                    // kaspa-pq H-05: revert an unbond request (clear the unbond clock).
                    BondMutation::Unbond(outpoint, _) => {
                        if let Ok(record) = store.get(&outpoint) {
                            let mut record = (*record).clone();
                            record.unbond_request_daa_score = None;
                            store.insert_batch(batch, outpoint, Arc::new(record)).unwrap();
                        }
                    }
                }
            }
        }

        // Apply blocks that joined the selected chain (in chain order).
        for added in chain_path.added.iter().copied() {
            for mutation in self.dns_bond_mutations_for_chain_block(added) {
                match mutation {
                    BondMutation::Insert(outpoint, record) => {
                        store.insert_batch(batch, outpoint, Arc::new(record)).unwrap();
                    }
                    BondMutation::Slash(outpoint, daa) => {
                        if let Ok(record) = store.get(&outpoint) {
                            let mut record = (*record).clone();
                            record.slashed_at_daa_score = Some(daa);
                            record.status = BondStatus::Slashed;
                            store.insert_batch(batch, outpoint, Arc::new(record)).unwrap();
                        }
                    }
                    // kaspa-pq H-05: apply an accepted unbond request (start the unbond clock).
                    BondMutation::Unbond(outpoint, daa) => {
                        if let Ok(record) = store.get(&outpoint) {
                            let mut record = (*record).clone();
                            record.unbond_request_daa_score = Some(daa);
                            store.insert_batch(batch, outpoint, Arc::new(record)).unwrap();
                        }
                    }
                }
            }
        }
    }

    /// Seeds the per-block [`ActiveBondView`] walk (ADR-0009 Addendum B §B.1)
    /// from the `StakeBonds` store snapshot — which, at the start of
    /// `resolve_virtual`, reflects the bond set as-of the previous sink (the
    /// same anchor `accumulated_diff` starts from). Returns an empty view on
    /// networks without the overlay (`dns_params` is `None`), so the bond-view
    /// walk is a no-op there.
    pub(super) fn initial_active_bond_view(&self) -> ActiveBondView {
        if self.dns_params.is_none() {
            return ActiveBondView::new();
        }
        ActiveBondView::from_records(
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (rec.bond_outpoint, (*rec).clone()))),
        )
    }

    /// Re-derives the [`BondMutation`]s a chain block contributed, from its
    /// retained acceptance data (ADR-0009 Addendum A.4). Deterministic, so it
    /// serves both apply (added) and revert (removed).
    fn dns_bond_mutations_for_chain_block(&self, chain_block: BlockHash) -> Vec<BondMutation> {
        let accepted_daa_score = self.headers_store.get_header(chain_block).unwrap().daa_score;
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        bond_mutations_from_accepted_txs(&self.accepted_txs_of_chain_block(chain_block), accepted_daa_score, min_bond, unbonding_floor)
    }

    /// The per-bond acceptance floors (min stake amount, min unbonding window) from the network's
    /// `DnsParams`, or `(0, 0)` where the overlay is off — so the bond-acceptance filter is a no-op
    /// on networks without `dns_params`.
    fn dns_bond_floors(&self) -> (u64, u64) {
        self.dns_params
            .as_ref()
            .map(|p| (p.min_bond_amount_sompi, p.unbonding_period_blocks))
            .unwrap_or((0, 0))
    }

    /// Resolves a chain block's accepted transactions from its acceptance data
    /// (`acceptance_data_store` → `block_transactions_store[index_within_block]`).
    /// Shared by the bond-population (A.4) and StakeScore-aggregation (A.5) passes,
    /// AND (with `--features evm`) the EVM lane.
    ///
    /// Tolerates missing acceptance data → no accepted transactions. A chain block has no committed
    /// acceptance data only when it is the imported pruning point (UTXO-set IBD writes the multiset
    /// but never acceptance data) or a pruned ancestor that a bounded backward overlay walk reaches.
    /// Every overlay reader funnels through here, so guarding the shared helper covers them all (the
    /// per-caller sink guard in `update_dns_state` was not enough: a NORMAL recompute walk legitimately
    /// reaches the pruning point). Returning empty is semantically correct — a block with no
    /// accountable acceptance data contributes no txs; a genuine inconsistency on a non-pruned block
    /// surfaces in the trace log instead of crashing the virtual processor.
    fn accepted_txs_of_chain_block(&self, chain_block: BlockHash) -> Vec<Transaction> {
        match self.acceptance_data_store.get(chain_block) {
            Ok(ad) => self.accepted_txs_from_acceptance_data(&ad),
            Err(StoreError::KeyNotFound(_)) => {
                trace!("accepted_txs_of_chain_block: no acceptance data for {chain_block} (pruning point / pruned) — treating as no accepted txs");
                Vec::new()
            }
            Err(e) => panic!("accepted_txs_of_chain_block: acceptance_data_store.get({chain_block}) failed: {e}"),
        }
    }

    /// Resolves accepted transactions from already-loaded acceptance data
    /// (`block_transactions_store[index_within_block]`). Split out so the
    /// per-block bond-view walk (ADR-0009 Addendum B) can derive a *not-yet-
    /// committed* block's mutations from the in-memory `ctx.mergeset_acceptance_data`,
    /// whose `acceptance_data_store` entry does not exist until `commit_utxo_state`.
    pub(super) fn accepted_txs_from_acceptance_data(&self, acceptance_data: &AcceptanceData) -> Vec<Transaction> {
        let mut txs = Vec::new();
        for mergeset in acceptance_data.iter() {
            let block_txs = self.block_transactions_store.get(mergeset.block_hash).unwrap();
            for entry in mergeset.accepted_transactions.iter() {
                if let Some(tx) = block_txs.get(entry.index_within_block as usize) {
                    txs.push(tx.clone());
                }
            }
        }
        txs
    }

    /// [`BondMutation`]s for a block whose acceptance data is held in-memory
    /// (the `KeyNotFound` chain block currently being UTXO-validated, before
    /// its `acceptance_data_store` entry is committed). Mirrors
    /// [`Self::dns_bond_mutations_for_chain_block`] but sources the accepted
    /// txs from the provided acceptance data instead of the store.
    fn dns_bond_mutations_from_acceptance(&self, acceptance_data: &AcceptanceData, accepted_daa_score: u64) -> Vec<BondMutation> {
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        bond_mutations_from_accepted_txs(&self.accepted_txs_from_acceptance_data(acceptance_data), accepted_daa_score, min_bond, unbonding_floor)
    }

    /// kaspa-pq Phase 10 (ADR-0009 Addendum A.5): recompute the DNS StakeScore
    /// over the bounded recent epoch window ending at `sink` and stage the
    /// updated [`DnsState`] singleton into `batch`. **Inert** unless the DNS
    /// overlay is configured (`dns_params.is_some()`).
    ///
    /// Bounded-window design (stake_depth is a window quantity, not cumulative):
    /// walk back at most `max_reorg_horizon_blocks` selected-chain blocks from
    /// `sink`, collect on-chain attestation shards, verify each ML-DSA-87
    /// signature against its bond's validator key under
    /// `ATTESTATION_MLDSA87_CONTEXT`, gate by `is_bond_active_at`, then feed the
    /// pure aggregation core. No new store; recompute is reorg-safe.
    fn update_dns_state(&self, batch: &mut WriteBatch, sink: BlockHash) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return;
        };
        // The StakeScore recompute below walks the selected chain reading each chain block's
        // acceptance data (`collect_stake_contributions_v2` -> `accepted_txs_of_chain_block`). During
        // pruning-point UTXO import (IBD), the sink IS the imported pruning point, whose acceptance
        // data is deliberately never written — `import_pruning_point_utxo_set` writes only the
        // multiset + UTXO status ("acceptance data and utxo-diff are irrelevant"). There is no chain
        // history to aggregate at that moment, so skip the recompute; `DnsState` is recompute-derived
        // and is rebuilt normally from the first fully-processed block after import. Without this
        // guard the walk panics with `KeyNotFound(AcceptanceData/<pruning point>)`, which surfaces as
        // a tokio runtime panic in the `spawn_blocking` import worker and crashes startup.
        match self.acceptance_data_store.get(sink) {
            Ok(_) => {}
            Err(StoreError::KeyNotFound(_)) => {
                // Missing acceptance data for the sink is EXPECTED only during pruning-point import,
                // where the sink IS the imported pruning point. Anywhere else it signals a store
                // inconsistency, so surface it loudly (still skip rather than panic, but never
                // silently): a genuine bug must be visible in the logs, not swallowed.
                let pp = self.pruning_point_store.read().pruning_point().optional().ok().flatten();
                if pp == Some(sink) {
                    trace!("update_dns_state: skipping recompute during pruning-point import (sink == pruning point {sink})");
                } else {
                    warn!(
                        "update_dns_state: acceptance data missing for sink {sink} (pruning point {pp:?}) — skipping DNS recompute; this is UNEXPECTED outside pruning-point import"
                    );
                }
                return;
            }
            Err(e) => panic!("update_dns_state: acceptance_data_store.get({sink}) failed: {e}"),
        }
        let sink_daa = self.headers_store.get_header(sink).unwrap().daa_score;
        // ADR-0009 Addendum A.3 network_id discriminator := the per-network genesis hash.
        let net_id = self.genesis.hash;

        // PR-10.11 throttle: StakeScore is per-epoch, so recompute DnsState only
        // once per epoch — when the sink's epoch differs from the last-written
        // DnsState's epoch. This bounds the window walk to ~once per
        // `epoch_length_blocks` (O(1) amortized per block) instead of walking
        // `max_reorg_horizon_blocks` on every virtual commit. Deterministic and
        // epoch-granular; safe on devnet/testnet where the gate is dormant
        // (Bootstrap). M-01 / audit #3: the recompute no longer depends on which sink first
        // crosses the boundary. The StakeScore is canonical (`collect_stake_contributions_v2`
        // credits only this chain's canonical lagged anchor per ready epoch), AND the
        // DNS-confirmed anchor is that canonical lagged anchor — NOT the sink (see
        // `confirmable_anchor` below). The reorg gate protects ONLY the confirmed anchor, so two
        // nodes that recompute at different boundary sinks still protect the identical anchor;
        // only `selected_chain_anchor` (read solely by this throttle) differs between them.
        let prev_dns_state = self.dns_state_store.read().get().ok();
        // kaspa-pq DNS v3: throttle the recompute to once per BLUE_SCORE epoch (epochs are
        // blue_score-coordinated now), not the DAA epoch. The recompute is canonical
        // regardless of cadence — this only bounds how often the window walk runs, and must
        // fire at least once per blue_score epoch so confirmations don't lag. `prev`'s
        // blue_score is read from its anchor (recent — at most ~1 epoch old, never pruned).
        let sink_blue = self.headers_store.get_blue_score(sink).unwrap();
        let epoch_len_blue = dns_params.attestation_epoch_length_blue_score.max(1);
        if let Some(prev) = prev_dns_state.as_ref() {
            let prev_blue = self.headers_store.get_blue_score(prev.selected_chain_anchor).unwrap_or(0);
            if sink_blue / epoch_len_blue == prev_blue / epoch_len_blue {
                return;
            }
        }

        // Snapshot the bond set (bounded by the active validator count).
        let bonds: Vec<StakeBondRecord> =
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();

        // Current total active stake + validator count at the sink (rollout gating).
        let mut total_active: u64 = 0;
        let mut active_validators: u32 = 0;
        for b in bonds.iter().filter(|b| is_bond_active_at(b, sink_daa)) {
            total_active = total_active.saturating_add(b.amount);
            active_validators = active_validators.saturating_add(1);
        }
        let rollout_stage = if sink_daa >= dns_params.dns_activation_daa_score
            && total_active >= dns_params.min_active_stake_sompi
            && active_validators >= dns_params.min_active_validators
            // kaspa-pq DNS v3 (PR6): refuse Active unless the blue_score canonical-anchor params
            // are self-consistent. In Active the reorg gate's finality depends entirely on them,
            // so an invalid config fails safe (stay Bootstrap, gate dormant) rather than splitting.
            && dns_params.dns_v3_params_consistent()
        {
            DnsRolloutStage::Active
        } else {
            DnsRolloutStage::Bootstrap
        };

        // kaspa-pq DNS v3: canonical, blue_score-coordinated StakeScore. Credit only
        // attestations naming THIS chain's canonical lagged anchor for their (ready,
        // non-duplicate) epoch, with the per-epoch denominator keyed by the canonical anchor
        // DAA and zero-attestation ready epochs included (`collect_stake_contributions_v2`).
        let (contributions, epoch_anchor_daa) =
            self.collect_stake_contributions_v2(sink, None, &bonds, net_id.as_byte_slice(), dns_params);

        let totals = total_active_stake_by_epoch(&bonds, &epoch_anchor_daa);
        let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
        let stake_depth = compute_stake_score(&per_epoch, dns_params.stake_event_quality_floor_bps);

        // kaspa-pq Phase 13 (ADR-0018 §C): derive the read-only DnsHealth liveness signal
        // from the same per-epoch tallies that fed the StakeScore. `overlay_active` iff the
        // reorg gate is engaged (`Active`); in Bootstrap there is no DNS finality to judge,
        // so health stays `DisabledBeforeActivation`. Purely a signal — never a
        // block-validity input, so this is inert wherever the gate is dormant.
        let health = derive_dns_health(
            &per_epoch,
            dns_params.stake_event_quality_floor_bps,
            dns_params.stake_censorship_floor_bps,
            dns_params.degraded_stake_quality_epochs,
            rollout_stage == DnsRolloutStage::Active,
        );

        // audit #3: the canonical lagged anchor of the latest ready epoch — a fixed,
        // blue_score-coordinated selected-chain point every node derives identically. THIS (not
        // the POV-dependent `sink`) is what gets DNS-confirmed and protected by the reorg gate, so
        // nodes that recompute at different boundary sinks still protect the same anchor. `None`
        // until an epoch's anchor is buried and lag-ready (early chain / not yet ready).
        let confirmable_anchor = ready_epoch_from_tip_blue_score(sink_blue, epoch_len_blue, dns_params.attestation_lag_blue_score)
            .and_then(|epoch| self.canonical_anchor_by_blue_score(epoch, sink, dns_params))
            .map(|a| (a.anchor_hash, a.anchor_daa_score));

        // true WorkDepth (audit H-02 Option A): WorkDepth(B) is the blue work accumulated SINCE the
        // confirmable anchor B — anchor-relative (`blue_work(sink) − blue_work(anchor)`), NOT the
        // cumulative-from-genesis `blue_work(sink)`. This makes it a real confirmation DEPTH (how much
        // PoW is piled on the confirmed point), so `is_dns_confirmed` genuinely requires BOTH a
        // work-depth AND a stake-depth (two-dimensional confirmation, matching the reorg gate's
        // anchor-relative work∧stake dominance). With `required_work_depth = 0` (devnet/simnet) this is
        // inert (stake-only); on mainnet/testnet (`required_work_depth > 0`) the work term gates too.
        // `ZERO` when no anchor is ready yet (no confirmation happens then anyway).
        let work_depth = confirmable_anchor
            .map(|(anchor_hash, _)| {
                self.ghostdag_store
                    .get_blue_work(sink)
                    .unwrap_or_default()
                    .saturating_sub(self.ghostdag_store.get_blue_work(anchor_hash).unwrap_or_default())
            })
            .unwrap_or_default();
        let new_state = advance_dns_confirmation(
            prev_dns_state.as_ref(),
            sink,
            sink_daa,
            confirmable_anchor,
            work_depth,
            stake_depth,
            rollout_stage,
            // validator_set_commitment: ADR-0017 dropped the sortition committee, so the
            // StakeScore path binds no committee snapshot — this stays zero.
            BlockHash::default(),
            health,
            dns_params.required_work_depth,
            dns_params.required_stake_depth,
        );
        self.dns_state_store.write().set_batch(batch, new_state).unwrap();
    }

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): recompute the per-epoch
    /// `EpochTally` accumulator over the bounded selected-chain window ending at
    /// `sink` and stage the live (non-finalized) epochs into `batch`. Gated by the
    /// v2 fence `pos_v2_activation_daa_score`: **inert** (returns after a single
    /// header read) on devnet/simnet (`GENESIS_ACTIVE_DNS_PARAMS`, fence `u64::MAX`);
    /// **active from block 1** on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence `0`)
    /// — also requires the DNS overlay to be configured.
    ///
    /// Recompute design (the `update_dns_state` precedent — reorg-safe with no
    /// incremental delta): the accumulator is a pure function of the selected
    /// chain (each block's persisted rewarded `(bond, epoch)` keys + quality
    /// sub-pool, both block-hash-keyed so only the current chain's rows are read)
    /// and the current bond snapshot, so a reorg simply re-derives the live epochs
    /// from the new chain.
    ///
    /// Window: `finalization_depth = reward_uniqueness_window_blocks +
    /// max_reorg_horizon_blocks` — a non-final epoch's included set stays mutable
    /// up to `window` past its anchor and a reorg can rewrite up to
    /// `max_reorg_horizon` blocks, so burying past their sum makes the tally
    /// immutable. The walk covers `finalization_depth + 2·epoch_length` so every
    /// non-final epoch's contributing blocks are seen. An epoch already `finalized`
    /// in the store is never re-derived (its blocks may lie partly outside the
    /// window — an incomplete recompute).
    ///
    /// NOTE (perf): unlike `update_dns_state` this does not throttle to
    /// once-per-epoch — instead the per-block work is **bounded by design** to the
    /// `walk_bound = finalization_depth + 2·epoch_length` window (a few thousand
    /// header/store reads at production params, all block-hash-keyed and cached), so
    /// it is O(window) per virtual commit, not O(chain). This bounded-window walk is
    /// what makes it reorg-safe (a pure function of the current selected chain, no
    /// incremental delta), and it runs from block 1 on mainnet/testnet (fence `0`).
    fn update_epoch_accumulator(&self, batch: &mut WriteBatch, sink: BlockHash) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return;
        };
        let sink_daa = self.headers_store.get_daa_score(sink).unwrap();
        // The v2 master fence: inert (no walk, no write) on devnet/simnet (`u64::MAX`);
        // the walk runs from block 1 on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence `0`).
        if sink_daa < dns_params.pos_v2_activation_daa_score {
            return;
        }

        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        let walk_bound = self.overlay_window_walk_bound(dns_params);

        // Gather this selected chain's per-block contributions within the window, oldest →
        // newest (so the `included` ordering is chain-deterministic). ADR-0022: this goes
        // through `selected_chain_overlay_window`, which merges the persisted below-pruning-
        // point window — so a pruned-IBD node recomputes epochs straddling the pruning point
        // correctly (its walk cannot reach below it). On a from-genesis node the merge is inert.
        let contributions: Vec<BlockEpochContribution> = self
            .selected_chain_overlay_window(sink, sink_daa, walk_bound)
            .into_iter()
            .map(|c| BlockEpochContribution { block_daa_score: c.block_daa_score, rewarded_keys: c.rewarded_keys, quality_subpool: c.quality_subpool })
            .collect();

        // Snapshot the bond set (bounded by the active validator count), as update_dns_state does.
        let bonds: Vec<StakeBondRecord> =
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();

        for (epoch, tally) in recompute_epoch_tallies(sink_daa, epoch_len, finalization_depth, &contributions, &bonds) {
            // Never re-derive a finalized epoch — it is immutable and its blocks may
            // already lie partly outside the walk window (an incomplete recompute).
            if self.epoch_accumulator_store.get(epoch).map(|t| t.finalized).unwrap_or(false) {
                continue;
            }
            self.epoch_accumulator_store.set_batch(batch, epoch, tally).unwrap();
        }
    }

    /// kaspa-pq ADR-0022: build the [`OverlaySnapshot`] **as-of `selected_parent`** —
    /// the exact set of overlay rows a pruned-IBD node needs to validate
    /// `selected_parent`'s descendants. Committed in `Header::overlay_commitment_root`
    /// (template fills it, `verify_expected_utxo_state` re-derives + checks it, c==v).
    ///
    /// Deterministic across the template path (`selected_parent` = sink) and the
    /// validation path (`selected_parent` = the block's selected parent): it reads
    /// only the walked bond view + per-block stores (`reserve_balance_store`,
    /// `rewarded_epochs_store`, `block_quality_pool_store`), never the per-sink
    /// epoch accumulator. Empty (⇒ `OverlaySnapshot::default()`) when the overlay
    /// is dormant; the window walk mirrors `update_epoch_accumulator` (same
    /// `walk_bound`, same pos_v2 fence) but is anchored at `selected_parent` and
    /// keeps only blocks that actually contributed (rewarded keys or quality pool),
    /// so the snapshot stays small on a validator-sparse chain.
    pub(super) fn compute_overlay_snapshot(&self, selected_parent: BlockHash, selected_parent_bond_view: &ActiveBondView) -> OverlaySnapshot {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return OverlaySnapshot::default();
        };

        let anchor_daa = self.headers_store.get_daa_score(selected_parent).unwrap();

        // Normalize the (non-canonical) stored `status` to the EFFECTIVE status at the
        // anchor. The raw `status` field diverges across reorg paths — `ActiveBondView::revert`
        // restores a reverted-slash bond to `Active` even if it was originally `Pending`, so a
        // never-slashed vs slashed-then-reverted bond can carry different `status` for byte-equal
        // history. `effective_bond_status` is a pure function of the canonical timing fields
        // (`activation_daa_score`/`slashed_at`/`unbond_request`), which the reward path already
        // uses; normalizing here makes the committed bond set deterministic across reorgs without
        // touching consensus-state mutation (the raw field is otherwise vestigial).
        let mut bonds = selected_parent_bond_view.records();
        for b in bonds.iter_mut() {
            b.status = effective_bond_status(b, anchor_daa);
        }
        let reserve_balance = self.reserve_balance_store.get(selected_parent).unwrap_or(0);

        let walk_bound = self.overlay_window_walk_bound(dns_params);
        let window = self.selected_chain_overlay_window(selected_parent, anchor_daa, walk_bound);

        OverlaySnapshot { bonds, reserve_balance, window }
    }

    /// ADR-0022: `reward_uniqueness_window + max_reorg_horizon + 2·epoch_length` — the
    /// selected-chain window that covers BOTH the reward-uniqueness dedup and the
    /// epoch-accumulator recompute. Shared by the overlay snapshot, the epoch
    /// accumulator, and the reward dedup so all three see the same span.
    pub(super) fn overlay_window_walk_bound(&self, dns_params: &DnsParams) -> u64 {
        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        finalization_depth.saturating_add(epoch_len.saturating_mul(2))
    }

    /// kaspa-pq ADR-0022: the per-block overlay contributions on `anchor`'s selected
    /// chain within `walk_bound` (rewarded keys + quality sub-pool), oldest → newest,
    /// MERGING the persisted pruning-point snapshot's below-pruning-point window.
    ///
    /// The selected-chain walk cannot traverse below the pruning point (no reachability
    /// there after a prune or a pruned-IBD import), so it stops at the persisted pruning
    /// point and the persisted snapshot supplies everything at/below it. On a node whose
    /// pruning point is far below `anchor` (normal operation) the walk never reaches it
    /// and every persisted entry is outside `walk_bound`, so the merge is a no-op
    /// (byte-identical to a from-genesis node). Empty-contribution blocks are skipped.
    /// The single seam through which all three below-pp consumers (overlay commitment,
    /// epoch accumulator, reward dedup) read the historical window.
    pub(super) fn selected_chain_overlay_window(&self, anchor: BlockHash, anchor_daa: u64, walk_bound: u64) -> Vec<BlockOverlayContribution> {
        let persisted = self.pruning_overlay_snapshot_store.read().get().ok();
        let stop_at = persisted.as_ref().map(|p| p.pruning_point);

        // Above-pruning-point part, collected newest → oldest by the chain walk.
        let mut above: Vec<BlockOverlayContribution> = Vec::new();
        for ancestor in std::iter::once(anchor).chain(self.reachability_service.default_backward_chain_iterator(anchor)) {
            if Some(ancestor) == stop_at {
                break;
            }
            let ancestor_daa = self.headers_store.get_daa_score(ancestor).unwrap();
            if anchor_daa.saturating_sub(ancestor_daa) > walk_bound {
                break;
            }
            let rewarded_keys = self.rewarded_epochs_store.get(ancestor).map(|k| (*k).clone()).unwrap_or_default();
            let quality_subpool = self.block_quality_pool_store.get(ancestor).unwrap_or(0);
            if rewarded_keys.is_empty() && quality_subpool == 0 {
                continue;
            }
            above.push(BlockOverlayContribution { block_hash: ancestor, block_daa_score: ancestor_daa, rewarded_keys, quality_subpool });
        }
        above.reverse(); // → oldest → newest

        // Below-pruning-point part: the persisted window (stored oldest → newest), kept
        // to entries still within `walk_bound` of the anchor. These never overlap `above`
        // (the walk stopped AT the pruning point), so prepending yields a single
        // oldest → newest selected-chain ordering.
        let mut window: Vec<BlockOverlayContribution> = Vec::new();
        if let Some(p) = persisted {
            for c in p.snapshot.window {
                if anchor_daa.saturating_sub(c.block_daa_score) <= walk_bound {
                    window.push(c);
                }
            }
        }
        window.extend(above);
        // kaspa-pq ADR-0022 fix: the persisted below-pruning-point window includes the pruning-point
        // boundary block (it is the newest entry of the captured `compute_overlay_snapshot(pp)` walk),
        // and across pruning advances that boundary block can also be re-captured into a later
        // snapshot's window — so a pruned-IBD node's recomputed window carried ONE EXTRA (duplicate)
        // entry at the pruning-point block vs a from-genesis node's clean live walk. That single extra
        // contribution changed the canonicalized overlay snapshot → the first post-pruning block's
        // `overlay_commitment_root` recompute (and the epoch/reward recompute that share this seam)
        // diverged (c != v) and the pruned-IBD node got stuck at "0 valid chain blocks". Dedup by block
        // hash: a from-genesis live walk visits each selected-chain block exactly once, so this is a
        // no-op there and only removes the spurious merge-path duplicate — restoring construction ==
        // validation for pruned-IBD joiners.
        let mut seen = std::collections::HashSet::new();
        window.retain(|c| seen.insert(c.block_hash));
        window
    }

    /// kaspa-pq Phase 13 (ADR-0018 §H) + DNS v3 (PR6): the StakeScore a branch accumulated
    /// **since the common ancestor** — the selected chain from `tip` back to (but excluding)
    /// `ancestor`, scored under `bonds` (that branch's bond set) and this network's `φS`. Uses
    /// the v3 canonical-anchor verifier (`collect_stake_contributions_v2`) with
    /// `stop_at = ancestor`, so the branch is scored only on canonical attestations for the
    /// epochs anchored strictly above the common ancestor (its OWN segment) — byte-identical to
    /// the sink-side StakeScore and immune to a branch inflating its score with non-canonical
    /// (current-sink / fabricated) targets. Inert wherever the overlay is dormant.
    fn stake_score_since_ancestor(
        &self,
        tip: BlockHash,
        ancestor: BlockHash,
        bonds: &[StakeBondRecord],
        dns_params: &DnsParams,
        net_id: &[u8],
    ) -> StakeScore {
        let (contributions, epoch_anchor_daa) =
            self.collect_stake_contributions_v2(tip, Some(ancestor), bonds, net_id, dns_params);
        let totals = total_active_stake_by_epoch(bonds, &epoch_anchor_daa);
        let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
        compute_stake_score(&per_epoch, dns_params.stake_event_quality_floor_bps)
    }

    /// kaspa-pq Phase 13 (ADR-0018 §H): the selected-chain common ancestor of `a` and `b`
    /// — the first block on `a`'s selected chain (from `a` inclusive, walking back) that is
    /// also a chain-ancestor of `b`. `None` if none is found within `max_walk` (a reorg
    /// deeper than the reorg horizon is not gate-eligible — the caller rejects it).
    fn selected_chain_common_ancestor(&self, a: BlockHash, b: BlockHash, max_walk: u64) -> Option<BlockHash> {
        let mut walked: u64 = 0;
        for block in std::iter::once(a).chain(self.reachability_service.default_backward_chain_iterator(a)) {
            if walked > max_walk {
                return None;
            }
            walked += 1;
            if self.reachability_service.is_chain_ancestor_of(block, b) {
                return Some(block);
            }
        }
        None
    }

    /// kaspa-pq DNS v3 (Canonical Lagged Anchor): the canonical, blue_score-coordinated
    /// epoch anchor for `epoch` as seen from `tip`'s selected chain — the **most-recent
    /// selected-chain ancestor with `blue_score <= anchor_cutoff(epoch)`** (cutoff =
    /// `epoch_end(epoch) - backoff`). Walks the selected-parent chain from `tip`
    /// (inclusive) reading each block's header `blue_score`/`daa_score`, collecting
    /// `(hash, blue_score, daa_score)` tip-first (blue_score strictly decreasing) until it
    /// buries the *previous* epoch's cutoff (so the pure core can decide the
    /// duplicate-anchor flag) or runs past `stake_score_window_blue_score`, then defers to
    /// the pure [`canonical_lagged_epoch_anchor`] core.
    ///
    /// The selected-chain *position* is read from header-committed `blue_score`, NEVER the
    /// store index (which is store-local: archival numbers from genesis, IBD from its
    /// pruning point), so archival and IBD-synced nodes derive the identical anchor. The
    /// signer (PR3), verifier (PR4), reward path (PR5) and reorg gate all call this so they
    /// agree on which block anchors an epoch. Reads only committed header data → reorg-safe.
    ///
    /// Returns `None` when the epoch's anchor cutoff is not yet buried by the tip
    /// (`cutoff > tip.blue_score` — a future / unburied epoch has no canonical anchor on
    /// this chain yet; the degenerate "most-recent-at-or-below == tip" is suppressed) or
    /// when the chain within the window does not reach the cutoff (epoch too old to
    /// credit). The stronger `attestation_lag_blue_score` readiness gate is applied by the
    /// signer / verifier on top of this.
    pub(crate) fn canonical_anchor_by_blue_score(
        &self,
        epoch: u64,
        tip: BlockHash,
        dns_params: &DnsParams,
    ) -> Option<CanonicalLaggedEpochAnchor> {
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let backoff = dns_params.attestation_anchor_backoff_blue_score;
        let window = dns_params.stake_score_window_blue_score;

        let tip_blue_score = self.headers_store.get_blue_score(tip).ok()?;
        // The epoch's anchor cutoff must be buried by the tip; otherwise "most-recent
        // at-or-below" would degenerate to the tip itself (a future / unburied epoch has no
        // canonical anchor on this chain yet).
        let cutoff = anchor_cutoff_blue_score(epoch, epoch_len, backoff);
        if cutoff > tip_blue_score {
            return None;
        }
        // Walk the selected-parent chain tip -> down, collecting (hash, blue, daa) until we
        // have buried the PREVIOUS epoch's cutoff (so the duplicate-anchor check is
        // decidable; for epoch 0 this coincides with this epoch's cutoff) or run past the
        // configured stake-score window. Position is read from blue_score, never the index.
        let needed = anchor_cutoff_blue_score(epoch.saturating_sub(1), epoch_len, backoff);
        let mut ancestors: Vec<(BlockHash, u64, u64)> = Vec::new();
        for hash in std::iter::once(tip).chain(self.reachability_service.default_backward_chain_iterator(tip)) {
            let compact = self.headers_store.get_compact_header_data(hash).ok()?;
            if tip_blue_score.saturating_sub(compact.blue_score) > window {
                break; // out of the stake-score window
            }
            ancestors.push((hash, compact.blue_score, compact.daa_score));
            if compact.blue_score <= needed {
                break; // buried the prev cutoff (and a fortiori this one) -> enough to decide
            }
        }
        canonical_lagged_epoch_anchor(epoch, epoch_len, backoff, &ancestors)
    }

    /// kaspa-pq DNS v3: the canonical anchors for every **creditable** epoch within the
    /// stake-score window ending at `tip`, computed in ONE selected-parent-chain walk.
    /// "Creditable" = ready (buried by `attestation_lag_blue_score`), non-duplicate
    /// (`anchor(E) != anchor(E-1)`; a sparse chain that reused the previous anchor earns no
    /// new credit), and recent enough that both `anchor_cutoff(E)` and `anchor_cutoff(E-1)`
    /// fall inside the collected window (so the duplicate flag is reliable). Older / unready
    /// / duplicate epochs are simply absent. Position comes from header-committed
    /// `blue_score`, never the store index, so archival and IBD-synced nodes agree.
    pub(super) fn canonical_anchors_in_window(&self, tip: BlockHash, dns_params: &DnsParams) -> BTreeMap<u64, CanonicalLaggedEpochAnchor> {
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let backoff = dns_params.attestation_anchor_backoff_blue_score;
        let lag = dns_params.attestation_lag_blue_score;
        let window = dns_params.stake_score_window_blue_score;

        let mut anchors: BTreeMap<u64, CanonicalLaggedEpochAnchor> = BTreeMap::new();
        let Ok(tip_blue) = self.headers_store.get_blue_score(tip) else {
            return anchors;
        };
        let Some(latest_ready) = ready_epoch_from_tip_blue_score(tip_blue, epoch_len, lag) else {
            return anchors; // no epoch buried by `lag` yet
        };

        // One walk: collect the selected chain tip-first down to the window bound.
        let mut ancestors: Vec<(BlockHash, u64, u64)> = Vec::new();
        for hash in std::iter::once(tip).chain(self.reachability_service.default_backward_chain_iterator(tip)) {
            let Ok(c) = self.headers_store.get_compact_header_data(hash) else {
                break;
            };
            if tip_blue.saturating_sub(c.blue_score) > window {
                break;
            }
            ancestors.push((hash, c.blue_score, c.daa_score));
        }
        let oldest_blue = ancestors.last().map(|a| a.1).unwrap_or(tip_blue);

        // From the latest ready epoch downward, derive each epoch's anchor over the shared
        // ancestor slice; stop once the PREVIOUS epoch's cutoff falls below the collected
        // window (older epochs aren't reliably decidable, hence not creditable). Skip
        // duplicates (no new credit).
        let mut epoch = latest_ready;
        loop {
            let prev_cutoff = anchor_cutoff_blue_score(epoch.saturating_sub(1), epoch_len, backoff);
            if prev_cutoff < oldest_blue {
                break;
            }
            if let Some(anchor) = canonical_lagged_epoch_anchor(epoch, epoch_len, backoff, &ancestors) {
                if !anchor.duplicate_of_previous_anchor {
                    anchors.insert(epoch, anchor);
                }
            }
            if epoch == 0 {
                break;
            }
            epoch -= 1;
        }
        anchors
    }

    /// kaspa-pq DNS v3 verifier: collect + verify the stake attestations on the selected
    /// chain ending at `tip`, crediting an attestation ONLY if it targets THIS chain's
    /// canonical anchor for its epoch (**GoodAttestation v3**): `att.target_hash` and
    /// `att.target_daa_score` equal the canonical `(anchor_hash, anchor_daa_score)` for
    /// `att.epoch`, the bond is `Active` at the canonical anchor DAA, the self-declared
    /// `validator_id` is bound to the bond (P-1A), and the ML-DSA-87 signature verifies under
    /// `ATTESTATION_MLDSA87_CONTEXT`. The per-epoch denominator (`epoch_anchor_daa`) is keyed
    /// by the CANONICAL anchor DAA (not the v1 first-seen self-reported value) and includes
    /// every creditable (ready, non-duplicate) epoch in the window — **even those with zero
    /// attestations** — so a participation gap is visible to φS / DnsHealth instead of
    /// silently vanishing (the v1 weakness that let honest validators signing divergent
    /// current-sink targets all fall below the φS floor).
    ///
    /// Replaces the v1 self-reported-target `collect_stake_contributions` for the sink-side
    /// StakeScore. For a branch segment (reorg gate, `stop_at = Some(I)`) it credits only
    /// epochs anchored strictly above the common ancestor `I` (the shared prefix belongs to
    /// neither branch's since-`I` delta); the reorg gate itself is migrated to this path in
    /// PR6 (it stays on v1 until then — inert, Active-only). Reads only committed acceptance
    /// + header data, so it is deterministic and reorg-safe; inert wherever the overlay is
    /// dormant.
    pub(super) fn collect_stake_contributions_v2(
        &self,
        tip: BlockHash,
        stop_at: Option<BlockHash>,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
    ) -> (Vec<AttestationContribution>, BTreeMap<u64, u64>) {
        // Canonical anchors for the creditable epoch window, computed from THIS chain's tip.
        let anchors = self.canonical_anchors_in_window(tip, dns_params);
        // For a branch segment (`stop_at = Some(I)`), credit only epochs anchored strictly
        // above `I`; the sink-side path (`None`) keeps them all.
        let creditable: BTreeMap<u64, CanonicalLaggedEpochAnchor> = anchors
            .into_iter()
            .filter(|(_, a)| match stop_at {
                Some(i) => a.anchor_hash != i && !self.reachability_service.is_chain_ancestor_of(a.anchor_hash, i),
                None => true,
            })
            .collect();
        let epoch_anchor_daa: BTreeMap<u64, u64> = creditable.iter().map(|(&e, a)| (e, a.anchor_daa_score)).collect();

        let mut contributions: Vec<AttestationContribution> = Vec::new();
        let Ok(tip_blue) = self.headers_store.get_blue_score(tip) else {
            return (contributions, epoch_anchor_daa);
        };
        for chain_block in self.reachability_service.default_backward_chain_iterator(tip) {
            if Some(chain_block) == stop_at {
                break;
            }
            let Ok(bs) = self.headers_store.get_blue_score(chain_block) else {
                break;
            };
            if tip_blue.saturating_sub(bs) > dns_params.stake_score_window_blue_score {
                break;
            }
            let txs = self.accepted_txs_of_chain_block(chain_block);
            for att in attestations_from_accepted_txs(&txs) {
                // v3 canonical gate: the attestation must name THIS chain's canonical anchor
                // for its epoch, and that epoch must be creditable (ready, non-duplicate,
                // in-window — i.e. present in `creditable`).
                let Some(anchor) = creditable.get(&att.epoch) else {
                    continue;
                };
                if att.target_hash != anchor.anchor_hash || att.target_daa_score != anchor.anchor_daa_score {
                    continue;
                }
                let Some(bond) = bonds.iter().find(|b| b.bond_outpoint == att.bond_outpoint) else {
                    continue;
                };
                // P-1A: the self-declared validator_id (not in the signed digest) must be
                // bound to the bond, else varying it would evade the dedup + inflate stake.
                if att.validator_id != bond.validator_pubkey_hash {
                    continue;
                }
                // The bond must be Active at the CANONICAL anchor DAA (== att.target_daa_score
                // by the gate above), not a self-reported / current value.
                if !is_bond_active_at(bond, anchor.anchor_daa_score) {
                    continue;
                }
                let digest = stake_attestation_message(
                    net_id,
                    att.epoch,
                    att.target_hash,
                    att.target_daa_score,
                    att.validator_set_commitment,
                    att.bond_outpoint,
                )
                .as_bytes();
                if matches!(
                    verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT),
                    Ok(true)
                ) {
                    contributions.push(AttestationContribution {
                        epoch: att.epoch,
                        validator_id: att.validator_id,
                        bond_outpoint: att.bond_outpoint,
                        signed_stake_sompi: bond.amount,
                    });
                }
            }
        }
        (contributions, epoch_anchor_daa)
    }

    /// kaspa-pq Phase 10/13 (ADR-0009 §"Decision" / ADR-0018 §H): the DNS finality reorg
    /// gate. Returns `true` (candidate sink allowed) unless the overlay is configured, in
    /// the `Active` rollout stage, has a confirmed anchor, and `candidate` would abandon
    /// that anchor's selected chain. **Inert** on every current network (`dns_params` is
    /// `None`) and outside the `Active` stage.
    ///
    /// `reorg_mode` (per-network, ADR-0018 §H) selects the rule when a candidate exits the
    /// confirmed prefix:
    /// - `HardCheckpoint` (PoC/testnet/devnet): reject any such exit.
    /// - `TwoDimensionalDominance` (mainnet): accept only if the candidate **strictly
    ///   out-Works AND out-Stakes** canonical since their common ancestor `I`, each by its
    ///   emergency margin (non-substitutability — neither dimension alone suffices).
    ///
    /// Safety: each branch's StakeScore-since-`I` is scored under **its own** bond set —
    /// `candidate_bond_view` (the sink-search view already advanced to `candidate`) for the
    /// candidate, and the persisted `stake_bonds_store` (still at `prev_sink`, because the
    /// bond store is written only at the final virtual commit, never during this sink
    /// search) for canonical. Scoring a branch under the wrong view could over-credit it
    /// and wrongly accept a confirmed-history-abandoning reorg. Both branches' acceptance
    /// data is committed by the time the gate runs (the candidate's by
    /// `calculate_utxo_state_relatively`), so the per-branch walks are deterministic.
    fn dns_reorg_allows(&self, candidate: BlockHash, prev_sink: BlockHash, candidate_bond_view: &ActiveBondView) -> bool {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return true;
        };
        let Ok(state) = self.dns_state_store.read().get() else {
            return true; // no DnsState written yet
        };
        if state.rollout_stage != DnsRolloutStage::Active {
            return true; // gate dormant outside the Active stage
        }
        let confirmed = state.last_dns_confirmed_anchor;
        if confirmed == BlockHash::default() {
            return true; // nothing confirmed yet
        }
        let includes = self.reachability_service.is_chain_ancestor_of(confirmed, candidate);

        // The heavy two-dimensional inputs (common ancestor + per-branch Work/Stake walks)
        // are computed ONLY when the candidate abandons the confirmed prefix AND the
        // network runs the mainnet dominance rule. HardCheckpoint and the includes-anchor
        // case ignore Work/Stake, so they skip the walks entirely.
        let inputs = if dns_params.reorg_mode == DnsReorgMode::TwoDimensionalDominance && !includes {
            // Selected-chain common ancestor I. Beyond the reorg horizon → not gate-eligible;
            // reject (a reorg deeper than the horizon cannot rewrite confirmed history).
            let Some(ancestor) = self.selected_chain_common_ancestor(candidate, prev_sink, dns_params.max_reorg_horizon_blocks)
            else {
                return false;
            };
            let net_id_hash = self.genesis.hash;
            let net_id = net_id_hash.as_byte_slice();
            // Per-branch bond sets (safety — each branch under its OWN view; see doc comment).
            let candidate_bonds = candidate_bond_view.records();
            let canonical_bonds: Vec<StakeBondRecord> =
                self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
            reorg_inputs_since_common_ancestor(
                state.rollout_stage,
                dns_params.reorg_mode,
                includes,
                self.ghostdag_store.get_blue_work(candidate).unwrap_or_default(),
                self.ghostdag_store.get_blue_work(prev_sink).unwrap_or_default(),
                self.ghostdag_store.get_blue_work(ancestor).unwrap_or_default(),
                self.stake_score_since_ancestor(candidate, ancestor, &candidate_bonds, dns_params, net_id),
                self.stake_score_since_ancestor(prev_sink, ancestor, &canonical_bonds, dns_params, net_id),
                dns_params.emergency_work_margin,
                dns_params.emergency_stake_margin,
            )
        } else {
            // HardCheckpoint, or candidate keeps the confirmed anchor: Work/Stake unused.
            reorg_inputs_since_common_ancestor(
                state.rollout_stage,
                dns_params.reorg_mode,
                includes,
                BlueWorkType::from_u64(0),
                BlueWorkType::from_u64(0),
                BlueWorkType::from_u64(0),
                StakeScore(0),
                StakeScore(0),
                dns_params.emergency_work_margin,
                dns_params.emergency_stake_margin,
            )
        };
        check_dns_reorg_rule(&inputs).is_accept()
    }

    /// Caches the DAA and Median time windows of the sink block (if needed). Following, virtual's window calculations will
    /// naturally hit the cache finding the sink's windows and building upon them.
    fn cache_sink_windows(
        &self,
        new_sink: BlockHash,
        prev_sink: BlockHash,
        sink_ghostdag_data: &impl Deref<Target = Arc<GhostdagData>>,
    ) {
        // We expect that the `new_sink` is cached (or some close-enough ancestor thereof) if it is equal to the `prev_sink`,
        // Hence we short-circuit the check of the keys in such cases, thereby reducing the access of the read-lock
        if new_sink != prev_sink {
            // this is only important for ibd performance, as we incur expensive cache misses otherwise.
            // this occurs because we cannot rely on header processing to pre-cache in this scenario.
            if !self.block_window_cache_for_difficulty.contains_key(&new_sink) {
                self.block_window_cache_for_difficulty
                    .insert(new_sink, self.window_manager.block_daa_window(sink_ghostdag_data.deref()).unwrap().window);
            };

            if !self.block_window_cache_for_past_median_time.contains_key(&new_sink) {
                self.block_window_cache_for_past_median_time
                    .insert(new_sink, self.window_manager.calc_past_median_time(sink_ghostdag_data.deref()).unwrap().1);
            };
        }
    }

    /// Returns the max number of tips to consider as virtual parents in a single virtual resolve operation.
    ///
    /// Guaranteed to be `>= self.max_block_parents`
    fn max_virtual_parent_candidates(&self, max_block_parents: usize) -> usize {
        // Limit to max_block_parents x 3 candidates. This way we avoid going over thousands of tips when the network isn't healthy.
        // There's no specific reason for a factor of 3, and its not a consensus rule, just an estimation for reducing the amount
        // of candidates considered.
        max_block_parents * 3
    }

    /// Searches for the next valid sink block (SINK = Virtual selected parent). The search is performed
    /// in the inclusive past of `tips`.
    /// The provided `diff` is assumed to initially hold the UTXO diff of `prev_sink` from virtual.
    /// The function returns with `diff` being the diff of the new sink from previous virtual.
    /// In addition to the found sink the function also returns a queue of additional virtual
    /// parent candidates ordered in descending blue work order.
    pub(super) fn sink_search_algorithm(
        &self,
        stores: &VirtualStores,
        diff: &mut UtxoDiff,
        bond_view: &mut ActiveBondView,
        prev_sink: BlockHash,
        tips: Vec<BlockHash>,
        finality_point: BlockHash,
        pruning_point: BlockHash,
    ) -> (BlockHash, VecDeque<BlockHash>) {
        // TODO (relaxed): additional tests

        let mut heap = tips
            .into_iter()
            .map(|block| SortableBlock { hash: block, blue_work: self.ghostdag_store.get_blue_work(block).unwrap() })
            .collect::<BinaryHeap<_>>();

        // The initial diff point is the previous sink
        let mut diff_point = prev_sink;

        // We maintain the following invariant: `heap` is an antichain.
        // It holds at step 0 since tips are an antichain, and remains through the loop
        // since we check that every pushed block is not in the past of current heap
        // (and it can't be in the future by induction)
        loop {
            let candidate = heap.pop().expect("valid sink must exist").hash;
            if self.reachability_service.is_chain_ancestor_of(finality_point, candidate) {
                diff_point = self.calculate_utxo_state_relatively(stores, diff, bond_view, diff_point, candidate);
                if diff_point == candidate {
                    // This indicates that candidate has valid UTXO state and that `diff` represents its diff from virtual

                    // kaspa-pq Phase 10 (ADR-0009): the DNS finality reorg gate. Inert
                    // unless the overlay is configured and in the Active stage; it then
                    // rejects a candidate that would abandon a DNS-confirmed anchor. The
                    // rejection is soft — we fall through to push the candidate's parents
                    // and continue, converging on a DNS-valid sink (mirrors the
                    // invalid-UTXO handling below).
                    if self.dns_reorg_allows(candidate, prev_sink, bond_view) {
                        // All blocks with lower blue work than filtering_root are:
                        // 1. not in its future (bcs blue work is monotonic),
                        // 2. will be removed eventually by the bounded merge check.
                        // Hence as an optimization we prefer removing such blocks in advance to allow valid tips to be considered.
                        let filtering_root = self.depth_store.merge_depth_root(candidate).unwrap();
                        let filtering_blue_work = self.ghostdag_store.get_blue_work(filtering_root).unwrap_or_default();
                        return (
                            candidate,
                            heap.into_sorted_iter().take_while(|s| s.blue_work >= filtering_blue_work).map(|s| s.hash).collect(),
                        );
                    }
                    debug!("Block candidate {} rejected by the DNS finality reorg gate; ignored from Virtual chain.", candidate);
                } else {
                    debug!("Block candidate {} has invalid UTXO state and is ignored from Virtual chain.", candidate)
                }
            } else if finality_point != pruning_point {
                // `finality_point == pruning_point` indicates we are at IBD start hence no warning required
                warn!("Finality Violation Detected. Block {} violates finality and is ignored from Virtual chain.", candidate);
            }
            // PRUNE SAFETY: see comment within [`resolve_virtual`]
            let prune_guard = self.pruning_lock.blocking_read();
            for parent in self.relations_service.get_parents(candidate).unwrap().iter().copied() {
                if self.reachability_service.is_dag_ancestor_of(finality_point, parent)
                    && !self.reachability_service.is_dag_ancestor_of_any(parent, &mut heap.iter().map(|sb| sb.hash))
                {
                    heap.push(SortableBlock { hash: parent, blue_work: self.ghostdag_store.get_blue_work(parent).unwrap() });
                }
            }
            drop(prune_guard);
        }
    }

    /// Picks the virtual parents according to virtual parent selection pruning constrains.
    /// Assumes:
    ///     1. `selected_parent` is a UTXO-valid block
    ///     2. `candidates` are an antichain ordered in descending blue work order
    ///     3. `candidates` do not contain `selected_parent` and `selected_parent.blue work > max(candidates.blue_work)`  
    pub(super) fn pick_virtual_parents(
        &self,
        selected_parent: BlockHash,
        mut candidates: VecDeque<BlockHash>,
        pruning_point: BlockHash,
    ) -> (Vec<BlockHash>, GhostdagData) {
        // TODO (relaxed): additional tests

        // Mergeset increasing might traverse DAG areas which are below the finality point and which theoretically
        // can borderline with pruned data, hence we acquire the prune lock to ensure data consistency. Note that
        // the final selected mergeset can never be pruned (this is the essence of the prunality proof), however
        // we might touch such data prior to validating the bounded merge rule. All in all, this function is short
        // enough so we avoid making further optimizations
        let _prune_guard = self.pruning_lock.blocking_read();
        let max_block_parents = self.max_block_parents as usize;
        let mergeset_size_limit = self.mergeset_size_limit;
        let max_candidates = self.max_virtual_parent_candidates(max_block_parents);

        // Prioritize half the blocks with highest blue work and pick the rest randomly to ensure diversity between nodes
        if candidates.len() > max_candidates {
            // make_contiguous should be a no op since the deque was just built
            let slice = candidates.make_contiguous();

            // Keep slice[..max_block_parents / 2] as is, choose max_candidates - max_block_parents / 2 in random
            // from the remainder of the slice while swapping them to slice[max_block_parents / 2..max_candidates].
            //
            // Inspired by rand::partial_shuffle (which lacks the guarantee on chosen elements location).
            for i in max_block_parents / 2..max_candidates {
                let j = rand::thread_rng().gen_range(i..slice.len()); // i < max_candidates < slice.len()
                slice.swap(i, j);
            }

            // Truncate the unchosen elements
            candidates.truncate(max_candidates);
        } else if candidates.len() > max_block_parents / 2 {
            // Fallback to a simpler algo in this case
            candidates.make_contiguous()[max_block_parents / 2..].shuffle(&mut rand::thread_rng());
        }

        let mut virtual_parents = Vec::with_capacity(min(max_block_parents, candidates.len() + 1));
        virtual_parents.push(selected_parent);
        let mut mergeset_size = 1; // Count the selected parent

        // Try adding parents as long as mergeset size and number of parents limits are not reached
        while let Some(candidate) = candidates.pop_front() {
            if mergeset_size >= mergeset_size_limit || virtual_parents.len() >= max_block_parents {
                break;
            }
            match self.mergeset_increase(&virtual_parents, candidate, mergeset_size_limit - mergeset_size) {
                MergesetIncreaseResult::Accepted { increase_size } => {
                    mergeset_size += increase_size;
                    virtual_parents.push(candidate);
                }
                MergesetIncreaseResult::Rejected { new_candidate } => {
                    // If we already have a candidate in the past of new candidate then skip.
                    if self.reachability_service.is_any_dag_ancestor(&mut candidates.iter().copied(), new_candidate) {
                        continue; // TODO (optimization): not sure this check is needed if candidates invariant as antichain is kept
                    }
                    // Remove all candidates which are in the future of the new candidate
                    candidates.retain(|&h| !self.reachability_service.is_dag_ancestor_of(new_candidate, h));
                    candidates.push_back(new_candidate);
                }
            }
        }
        assert!(mergeset_size <= mergeset_size_limit);
        assert!(virtual_parents.len() <= max_block_parents);
        self.remove_bounded_merge_breaking_parents(virtual_parents, pruning_point)
    }

    fn mergeset_increase(&self, selected_parents: &[BlockHash], candidate: BlockHash, budget: u64) -> MergesetIncreaseResult {
        /*
        Algo:
            Traverse past(candidate) \setminus past(selected_parents) and make
            sure the increase in mergeset size is within the available budget
        */

        let candidate_parents = self.relations_service.get_parents(candidate).unwrap();
        let mut queue: VecDeque<_> = candidate_parents.iter().copied().collect();
        let mut visited: BlockHashSet = queue.iter().copied().collect();
        let mut mergeset_increase = 1u64; // Starts with 1 to count for the candidate itself

        while let Some(current) = queue.pop_front() {
            if self.reachability_service.is_dag_ancestor_of_any(current, &mut selected_parents.iter().copied()) {
                continue;
            }
            mergeset_increase += 1;
            if mergeset_increase > budget {
                return MergesetIncreaseResult::Rejected { new_candidate: current };
            }

            let current_parents = self.relations_service.get_parents(current).unwrap();
            for &parent in current_parents.iter() {
                if visited.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }
        MergesetIncreaseResult::Accepted { increase_size: mergeset_increase }
    }

    fn remove_bounded_merge_breaking_parents(
        &self,
        mut virtual_parents: Vec<BlockHash>,
        current_pruning_point: BlockHash,
    ) -> (Vec<BlockHash>, GhostdagData) {
        let mut ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);
        let merge_depth_root = self.depth_manager.calc_merge_depth_root(&ghostdag_data, current_pruning_point);
        let mut kosherizing_blues: Option<Vec<BlockHash>> = None;
        let mut bad_reds = Vec::new();

        //
        // Note that the code below optimizes for the usual case where there are no merge-bound-violating blocks.
        //

        // Find red blocks violating the merge bound and which are not kosherized by any blue
        for red in ghostdag_data.mergeset_reds.iter().copied() {
            if self.reachability_service.is_dag_ancestor_of(merge_depth_root, red) {
                continue;
            }
            // Lazy load the kosherizing blocks since this case is extremely rare
            if kosherizing_blues.is_none() {
                kosherizing_blues = Some(self.depth_manager.kosherizing_blues(&ghostdag_data, merge_depth_root).collect());
            }
            if !self.reachability_service.is_dag_ancestor_of_any(red, &mut kosherizing_blues.as_ref().unwrap().iter().copied()) {
                bad_reds.push(red);
            }
        }

        if !bad_reds.is_empty() {
            // Remove all parents which lead to merging a bad red
            virtual_parents.retain(|&h| !self.reachability_service.is_any_dag_ancestor(&mut bad_reds.iter().copied(), h));
            // Recompute ghostdag data since parents changed
            ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);
        }

        (virtual_parents, ghostdag_data)
    }

    fn validate_mempool_transaction_impl(
        &self,
        mutable_tx: &mut MutableTransaction,
        virtual_utxo_view: &impl UtxoView,
        virtual_daa_score: u64,
        virtual_past_median_time: u64,
        args: &TransactionValidationArgs,
    ) -> TxResult<()> {
        self.transaction_validator.validate_tx_in_isolation(&mutable_tx.tx)?;
        self.transaction_validator.validate_tx_in_header_context_with_args(
            &mutable_tx.tx,
            virtual_daa_score,
            virtual_past_median_time,
        )?;
        self.validate_mempool_transaction_in_utxo_context(mutable_tx, virtual_utxo_view, virtual_daa_score, args)?;
        Ok(())
    }

    pub fn validate_mempool_transaction(&self, mutable_tx: &mut MutableTransaction, args: &TransactionValidationArgs) -> TxResult<()> {
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let virtual_utxo_view = &virtual_read.utxo_set;
        let virtual_daa_score = virtual_state.daa_score;
        let virtual_past_median_time = virtual_state.past_median_time;
        // Run within the thread pool since par_iter might be internally applied to inputs
        self.thread_pool.install(|| {
            self.validate_mempool_transaction_impl(mutable_tx, virtual_utxo_view, virtual_daa_score, virtual_past_median_time, args)
        })
    }

    pub fn validate_mempool_transactions_in_parallel(
        &self,
        mutable_txs: &mut [MutableTransaction],
        args: &TransactionValidationBatchArgs,
    ) -> Vec<TxResult<()>> {
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let virtual_utxo_view = &virtual_read.utxo_set;
        let virtual_daa_score = virtual_state.daa_score;
        let virtual_past_median_time = virtual_state.past_median_time;

        self.thread_pool.install(|| {
            mutable_txs
                .par_iter_mut()
                .map(|mtx| {
                    self.validate_mempool_transaction_impl(
                        mtx,
                        &virtual_utxo_view,
                        virtual_daa_score,
                        virtual_past_median_time,
                        args.get(&mtx.id()),
                    )
                })
                .collect::<Vec<TxResult<()>>>()
        })
    }

    fn populate_mempool_transaction_impl(
        &self,
        mutable_tx: &mut MutableTransaction,
        virtual_utxo_view: &impl UtxoView,
    ) -> TxResult<()> {
        self.populate_mempool_transaction_in_utxo_context(mutable_tx, virtual_utxo_view)?;
        Ok(())
    }

    pub fn populate_mempool_transaction(&self, mutable_tx: &mut MutableTransaction) -> TxResult<()> {
        let virtual_read = self.virtual_stores.read();
        let virtual_utxo_view = &virtual_read.utxo_set;
        self.populate_mempool_transaction_impl(mutable_tx, virtual_utxo_view)
    }

    pub fn populate_mempool_transactions_in_parallel(&self, mutable_txs: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        let virtual_read = self.virtual_stores.read();
        let virtual_utxo_view = &virtual_read.utxo_set;
        self.thread_pool.install(|| {
            mutable_txs
                .par_iter_mut()
                .map(|mtx| self.populate_mempool_transaction_impl(mtx, &virtual_utxo_view))
                .collect::<Vec<TxResult<()>>>()
        })
    }

    fn validate_block_template_transactions_in_parallel<V: UtxoView + Sync>(
        &self,
        txs: &[Transaction],
        virtual_state: &VirtualState,
        utxo_view: &V,
    ) -> Vec<TxResult<u64>> {
        self.thread_pool
            .install(|| txs.par_iter().map(|tx| self.validate_block_template_transaction(tx, virtual_state, &utxo_view)).collect())
    }

    fn validate_block_template_transaction(
        &self,
        tx: &Transaction,
        virtual_state: &VirtualState,
        utxo_view: &impl UtxoView,
    ) -> TxResult<u64> {
        // No need to validate the transaction in isolation since we rely on the mining manager to submit transactions
        // which were previously validated through `validate_mempool_transaction_and_populate`, hence we only perform
        // in-context validations
        self.transaction_validator.validate_tx_in_header_context_with_args(
            tx,
            virtual_state.daa_score,
            virtual_state.past_median_time,
        )?;
        let ValidatedTransaction { calculated_fee, .. } =
            self.validate_transaction_in_utxo_context(tx, utxo_view, virtual_state.daa_score, TxValidationFlags::Full)?;
        Ok(calculated_fee)
    }

    pub fn build_block_template(
        &self,
        miner_data: MinerData,
        mut tx_selector: Box<dyn TemplateTransactionSelector>,
        build_mode: TemplateBuildMode,
        // kaspa-pq EVM Lane v0.4 (§15 step 6 / §16): the node's own payload
        // candidates + declared EVM coinbase. Assembled into the template
        // payload by `evm_template_fields`; ignored pre-activation.
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        //
        // TODO (relaxed): additional tests
        //

        // We call for the initial tx batch before acquiring the virtual read lock,
        // optimizing for the common case where all txs are valid. Following selection calls
        // are called within the lock in order to preserve validness of already validated txs
        let mut txs = tx_selector.select_transactions();
        let mut calculated_fees = Vec::with_capacity(txs.len());
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let virtual_utxo_view = &virtual_read.utxo_set;

        let mut invalid_transactions = HashMap::new();
        let results = self.validate_block_template_transactions_in_parallel(&txs, &virtual_state, &virtual_utxo_view);
        for (tx, res) in txs.iter().zip(results) {
            match res {
                Err(e) => {
                    invalid_transactions.insert(tx.id(), e);
                    tx_selector.reject_selection(tx.id());
                }
                Ok(fee) => {
                    calculated_fees.push(fee);
                }
            }
        }

        let mut has_rejections = !invalid_transactions.is_empty();
        if has_rejections {
            txs.retain(|tx| !invalid_transactions.contains_key(&tx.id()));
        }

        while has_rejections {
            has_rejections = false;
            let next_batch = tx_selector.select_transactions(); // Note that once next_batch is empty the loop will exit
            let next_batch_results =
                self.validate_block_template_transactions_in_parallel(&next_batch, &virtual_state, &virtual_utxo_view);
            for (tx, res) in next_batch.into_iter().zip(next_batch_results) {
                match res {
                    Err(e) => {
                        invalid_transactions.insert(tx.id(), e);
                        tx_selector.reject_selection(tx.id());
                        has_rejections = true;
                    }
                    Ok(fee) => {
                        txs.push(tx);
                        calculated_fees.push(fee);
                    }
                }
            }
        }

        // Check whether this was an overall successful selection episode. We pass this decision
        // to the selector implementation which has the broadest picture and can use mempool config
        // and context
        match (build_mode, tx_selector.is_successful()) {
            (TemplateBuildMode::Standard, false) => return Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions)),
            (TemplateBuildMode::Standard, true) | (TemplateBuildMode::Infallible, _) => {}
        }

        // kaspa-pq narrow P0-1: capture the bond view AND materialize the
        // deposit-claim lock entries in the SAME virtual generation as
        // `virtual_state` (= the template's selected parent), BEFORE releasing the
        // read lock. The overlay commitment and claim validation then use one
        // coherent generation, never a later re-read of a possibly-advanced view
        // (the mixed-generation TOCTOU). `virtual_state.daa_score` is exactly the
        // template header's daa_score (see `Header::new_finalized` below).
        let template_bond_view = self.initial_active_bond_view();
        let prepared_claims = crate::processes::evm::prepare_deposit_claims(
            &evm_template_data.system_ops,
            virtual_utxo_view,
            virtual_state.daa_score,
        );

        // At this point we can safely drop the read lock
        drop(virtual_read);

        // Build the template
        self.build_block_template_from_virtual_state(
            virtual_state,
            template_bond_view,
            prepared_claims,
            miner_data,
            txs,
            calculated_fees,
            evm_template_data,
        )
    }

    pub(crate) fn validate_block_template_transactions(
        &self,
        txs: &[Transaction],
        virtual_state: &VirtualState,
        utxo_view: &impl UtxoView,
    ) -> Result<(), RuleError> {
        // Search for invalid transactions
        let mut invalid_transactions = HashMap::new();
        for tx in txs.iter() {
            if let Err(e) = self.validate_block_template_transaction(tx, virtual_state, utxo_view) {
                invalid_transactions.insert(tx.id(), e);
            }
        }
        if !invalid_transactions.is_empty() { Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions)) } else { Ok(()) }
    }

    pub(crate) fn build_block_template_from_virtual_state(
        &self,
        virtual_state: Arc<VirtualState>,
        // kaspa-pq narrow P0-1: the bond view + deposit-claim snapshot, both
        // captured in the SAME virtual generation as `virtual_state` by the caller
        // (under one read lock) — so the reward fan-out, the overlay commitment and
        // the EVM claim payload all reference one coherent generation.
        template_bond_view: ActiveBondView,
        prepared_claims: crate::processes::evm::PreparedDepositClaims,
        miner_data: MinerData,
        mut txs: Vec<Transaction>,
        calculated_fees: Vec<u64>,
        // kaspa-pq EVM Lane v0.4 (§15 step 6 / §16): own-payload inputs.
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        // [`calc_block_parents`] can use deep blocks below the pruning point for this calculation, so we
        // need to hold the pruning lock.
        let _prune_guard = self.pruning_lock.blocking_read();
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let header_pruning_point =
            self.pruning_point_manager.expected_header_pruning_point(virtual_state.ghostdag_data.to_compact()).pruning_point;
        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4/§B.5): the validator
        // reward fan-out for this template. The template extends the current
        // tip, so the bond set as-of its selected parent is the `StakeBonds`
        // store snapshot (= state at the sink) — `initial_active_bond_view`.
        // First drop any ineligible attestation shard so the mined block passes
        // the §B.4 rule; then compute the reward outputs with the SAME
        // `validator_reward_outputs_for_block` the validation path uses, so a
        // block mined from this template reproduces the coinbase byte-for-byte.
        // Both are no-ops on every current network (overlay dormant). The bond
        // view is now captured by the caller in the template's virtual generation
        // (narrow P0-1) and passed in, not re-read here.
        self.retain_reward_eligible_attestation_shards(&mut txs, &template_bond_view, virtual_state.daa_score);
        // kaspa-pq Phase 13 (ADR-0018 §F+§E): the §F carve + §E validator pool for
        // this template, computed identically to the validation path so a block
        // mined from this template reproduces the coinbase byte-for-byte. `None`/0
        // on every current network (overlay dormant).
        // ADR-0018 §F staged rollout: None (Stage 1) / bootstrap (Stage 2) / full
        // (Stage 3) selected by DAA, identically to the validation path.
        let carve = self.dns_params.as_ref().and_then(|p| p.reward_fee_split(virtual_state.daa_score));
        let validator_pool = carve.map_or(0, |fs| {
            self.coinbase_manager.coinbase_validator_pool(
                &virtual_state.ghostdag_data,
                &virtual_state.mergeset_rewards,
                &virtual_state.mergeset_non_daa,
                fs,
            )
        });
        let (validator_reward_outputs, _rewarded_keys, newly_included_stake, expected_stake) = self.validator_reward_outputs_for_block(
            &txs,
            &template_bond_view,
            virtual_state.daa_score,
            virtual_state.ghostdag_data.selected_parent,
            validator_pool,
        );
        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): append the reserve-drip outputs so a block
        // mined from this template reproduces the validated coinbase byte-for-byte. Reads the sink's
        // committed reserve balance (= the template's selected parent). Inert below the v2 fence.
        let mut validator_reward_outputs = validator_reward_outputs;
        if let Some(dns_params) = self.dns_params.as_ref() {
            let parent_balance = self.reserve_balance_store.get(virtual_state.ghostdag_data.selected_parent).unwrap_or(0);
            let (drip_outputs, _) = self.reserve_drip_outputs(
                dns_params,
                virtual_state.daa_score,
                virtual_state.ghostdag_data.selected_parent,
                &template_bond_view,
                parent_balance,
            );
            validator_reward_outputs.extend(drip_outputs);
        }
        let coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(
                virtual_state.daa_score,
                miner_data.clone(),
                &virtual_state.ghostdag_data,
                &virtual_state.mergeset_rewards,
                &virtual_state.mergeset_non_daa,
                &validator_reward_outputs,
                carve,
                (newly_included_stake, expected_stake),
            )
            .unwrap();
        txs.insert(0, coinbase.tx);
        // kaspa-pq EVM Lane v0.4 (§4.3/§15): the template declares the
        // fork-correct header version — v2 (two EVM commitments) at/after
        // activation, v1 before (mirrors the check_header_version rule).
        let version = if virtual_state.daa_score >= self.evm_activation_daa_score {
            kaspa_consensus_core::constants::EVM_HEADER_VERSION
        } else {
            BLOCK_VERSION
        };
        let parents_by_level = self.parents_manager.calc_block_parents(pruning_point, &virtual_state.parents);
        let hash_merkle_root = calc_hash_merkle_root(txs.iter());

        let accepted_id_merkle_root = self
            .calc_accepted_id_merkle_root(virtual_state.accepted_tx_ids.iter().copied(), virtual_state.ghostdag_data.selected_parent);
        let utxo_commitment = virtual_state.multiset.clone().finalize();
        // Past median time is the exclusive lower bound for valid block time, so we increase by 1 to get the valid min
        let min_block_time = virtual_state.past_median_time + 1;
        let header = Header::new_finalized(
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            u64::max(min_block_time, unix_now()),
            virtual_state.bits,
            0,
            // kaspa-pq Phase 3 (ADR-0007): the template declares the network-correct Layer-1 algo
            // for this DAA score — BLAKE2b-512 ∥ SHA3-512 (algo_id = 3) once activated, else kHeavyHash (1).
            kaspa_consensus_core::pow_layer0::required_algo_id(
                self.pow_blake2b_sha3_activation.is_active(virtual_state.daa_score),
            ),
            virtual_state.daa_score,
            virtual_state.ghostdag_data.blue_work,
            virtual_state.ghostdag_data.blue_score,
            header_pruning_point,
        );
        // kaspa-pq EVM Lane v0.4 (§15): on an evm-active template, execute the
        // mergeset acceptance NOW (the producer-side run of the exact verifier
        // code) and commit both EVM header fields. The own payload is empty
        // until the EVM mempool lands (§16 phase) — its (non-zero) hash is
        // still committed. Inert (returns the header unchanged) pre-activation.
        let (header, evm_payload, stale_evm_claims) =
            self.evm_template_fields(header, &virtual_state, evm_template_data, prepared_claims)?;
        // kaspa-pq ADR-0022: commit the DNS/PoS-v2 overlay snapshot as-of the template's
        // selected parent (the sink) — the SAME `compute_overlay_snapshot` the validation
        // path re-derives, so a block mined from this template reproduces the
        // `overlay_commitment_root` byte-for-byte (construction == validation). Inert
        // (header unchanged) when the overlay is dormant. Appended after the EVM fields;
        // `with_overlay_commitment` re-finalizes over the full preimage.
        let header = if self.dns_params.is_some() {
            let overlay_root =
                self.compute_overlay_snapshot(virtual_state.ghostdag_data.selected_parent, &template_bond_view).commitment_root();
            header.with_overlay_commitment(overlay_root)
        } else {
            header
        };
        let selected_parent_hash = virtual_state.ghostdag_data.selected_parent;
        let selected_parent_timestamp = self.headers_store.get_timestamp(selected_parent_hash).unwrap();
        let selected_parent_daa_score = self.headers_store.get_daa_score(selected_parent_hash).unwrap();
        let mut template_block = MutableBlock::new(header, txs);
        template_block.evm_payload = evm_payload;
        Ok(BlockTemplate::new(
            template_block,
            miner_data,
            coinbase.has_red_reward,
            coinbase.miner_script_output_indices,
            selected_parent_timestamp,
            selected_parent_daa_score,
            selected_parent_hash,
            calculated_fees,
            stale_evm_claims,
        ))
    }

    /// Make sure pruning point-related stores are initialized
    pub fn init(self: &Arc<Self>) {
        let pruning_point_read = self.pruning_point_store.upgradable_read();
        if pruning_point_read.pruning_point().optional().unwrap().is_none() {
            let mut pruning_point_write = RwLockUpgradableReadGuard::upgrade(pruning_point_read);
            let mut pruning_meta_write = self.pruning_meta_stores.write();
            let mut batch = WriteBatch::default();
            self.past_pruning_points_store.insert_batch(&mut batch, 0, self.genesis.hash).idempotent().unwrap();
            pruning_point_write.set_batch(&mut batch, self.genesis.hash, 0).unwrap();
            pruning_point_write.set_retention_checkpoint(&mut batch, self.genesis.hash).unwrap();
            pruning_point_write.set_retention_period_root(&mut batch, self.genesis.hash).unwrap();
            pruning_meta_write.set_utxoset_position(&mut batch, self.genesis.hash).unwrap();
            self.db.write(batch).unwrap();
            drop(pruning_point_write);
            drop(pruning_meta_write);
        }
    }

    /// Initializes UTXO state of genesis and points virtual at genesis.
    /// Note that pruning point-related stores are initialized by `init`
    pub fn process_genesis(self: &Arc<Self>) {
        // Write the UTXO state of genesis
        self.commit_utxo_state(
            self.genesis.hash,
            UtxoDiff::default(),
            MuHash::new(),
            AcceptanceData::default(),
            ZERO_HASH64,
            Vec::new(),
            0, // kaspa-pq ADR-0018 "本格版": genesis has no validator quality sub-pool.
            0, // kaspa-pq ADR-0018 "本格版" (Phase 4): genesis reserve balance is 0.
            None, // kaspa-pq ADR-0020 v0.4: genesis is EVM-inert (v0 header).
        );

        // Init the virtual selected chain store
        let mut batch = WriteBatch::default();
        let mut selected_chain_write = self.selected_chain_store.write();
        selected_chain_write.init_with_pruning_point(&mut batch, self.genesis.hash).unwrap();
        self.db.write(batch).unwrap();
        drop(selected_chain_write);

        // Init virtual state
        self.commit_virtual_state(
            self.virtual_stores.upgradable_read(),
            Arc::new(VirtualState::from_genesis(&self.genesis, self.ghostdag_manager.ghostdag(&[self.genesis.hash]))),
            &Default::default(),
            &Default::default(),
        );
    }

    /// Finalizes the pruning point utxoset state and imports the pruning point utxoset *to* virtual utxoset
    pub fn import_pruning_point_utxo_set(
        &self,
        new_pruning_point: BlockHash,
        mut imported_utxo_multiset: MuHash,
    ) -> PruningImportResult<()> {
        info!("Importing the UTXO set of the pruning point {}", new_pruning_point);
        let new_pruning_point_header = self.headers_store.get_header(new_pruning_point).unwrap();
        let imported_utxo_multiset_hash = imported_utxo_multiset.finalize();
        if imported_utxo_multiset_hash != new_pruning_point_header.utxo_commitment {
            return Err(PruningImportError::ImportedMultisetHashMismatch(
                new_pruning_point_header.utxo_commitment,
                imported_utxo_multiset_hash,
            ));
        }

        {
            // Set the pruning point utxoset position to the new point we just verified
            let mut batch = WriteBatch::default();
            let mut pruning_meta_write = self.pruning_meta_stores.write();
            pruning_meta_write.set_utxoset_position(&mut batch, new_pruning_point).unwrap();
            self.db.write(batch).unwrap();
            drop(pruning_meta_write);
        }

        {
            // Copy the pruning-point UTXO set into virtual's UTXO set
            let pruning_meta_read = self.pruning_meta_stores.read();
            let mut virtual_write = self.virtual_stores.write();

            virtual_write.utxo_set.clear().unwrap();
            for chunk in &pruning_meta_read.utxo_set.iterator().map(|iter_result| iter_result.unwrap()).chunks(1000) {
                virtual_write.utxo_set.write_from_iterator_without_cache(chunk).unwrap();
            }
        }

        let virtual_read = self.virtual_stores.upgradable_read();

        // Validate transactions of the pruning point itself
        let new_pruning_point_transactions = self.block_transactions_store.get(new_pruning_point).unwrap();
        let validated_transactions = self.validate_transactions_in_parallel(
            &new_pruning_point_transactions,
            &virtual_read.utxo_set,
            new_pruning_point_header.daa_score,
            TxValidationFlags::Full,
        );
        if validated_transactions.len() < new_pruning_point_transactions.len() - 1 {
            // Some non-coinbase transactions are invalid
            return Err(PruningImportError::NewPruningPointTxErrors);
        }

        {
            // Submit partial UTXO state for the pruning point.
            // Note we only have and need the multiset; acceptance data and utxo-diff are irrelevant.
            let mut batch = WriteBatch::default();
            self.utxo_multisets_store.set_batch(&mut batch, new_pruning_point, imported_utxo_multiset.clone()).unwrap();

            let statuses_write = self.statuses_store.set_batch(&mut batch, new_pruning_point, StatusUTXOValid).unwrap();
            self.db.write(batch).unwrap();
            drop(statuses_write);
        }

        // Calculate the virtual state, treating the pruning point as the only virtual parent
        let virtual_parents = vec![new_pruning_point];
        let virtual_ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);

        self.calculate_and_commit_virtual_state(
            virtual_read,
            virtual_parents,
            virtual_ghostdag_data,
            imported_utxo_multiset.clone(),
            &mut UtxoDiff::default(),
            // Pruning-point UTXO import (IBD): the `StakeBonds` store snapshot is
            // the bond set as-of the imported pruning point. Empty on every
            // current network (overlay dormant), so this is inert.
            &self.initial_active_bond_view(),
            &ChainPath::default(),
        )?;

        Ok(())
    }

    /// kaspa-pq ADR-0022: import the pruning point's EVM execution state during
    /// headers-proof IBD. Without this, the first post-pruning block re-executes the
    /// EVM lane against an empty genesis state (the pruning point has no
    /// `evm_header_store` row on a fresh node), so its recomputed `evm_commitment_root`
    /// mismatches the header and the whole chain is disqualified.
    ///
    /// Verification (trustless): the supplied [`EvmExecutionHeader`] must reproduce
    /// the L1 header's `evm_commitment_root` (a pure, secp-free keyed-BLAKE2b check),
    /// and — on an `evm` build — the supplied [`EvmStateSnapshot`] must reproduce that
    /// EVM header's `state_root` (the keccak-MPT root over the account set). Then the
    /// two rows are persisted and the canonical **finalized** EVM head is set to the
    /// pruning point, so `evm_execute_acceptance_with_parent` finds the real parent
    /// state for `pp`'s children.
    pub fn import_pruning_point_evm_state(
        &self,
        pruning_point: BlockHash,
        evm_header: kaspa_consensus_core::evm::EvmExecutionHeader,
        snapshot: kaspa_consensus_core::evm::EvmStateSnapshot,
    ) -> PruningImportResult<()> {
        info!("Importing the EVM state of the pruning point {}", pruning_point);
        let l1_header = self.headers_store.get_header(pruning_point).unwrap();

        // (1) The EVM header must reproduce the L1 commitment (pure; works on any build).
        let got = evm_header.commitment_root();
        if got != l1_header.evm_commitment_root {
            return Err(PruningImportError::ImportedEvmCommitmentMismatch(pruning_point, got, l1_header.evm_commitment_root));
        }

        // (2) The state snapshot must reproduce the EVM header's keccak-MPT state root.
        // Requires the EVM executor; an `evm`-active network can only be synced by an
        // `--features evm` build (a default build rejects its v2 headers earlier), so
        // skipping this on a non-evm build never weakens a chain it actually follows.
        #[cfg(feature = "evm")]
        {
            let db = kaspa_evm::snapshot::seed_cachedb(&snapshot)
                .map_err(|e| PruningImportError::ImportedEvmSnapshotInvalid(pruning_point, e.to_string()))?;
            let computed = kaspa_hashes::EvmH256::from_bytes(kaspa_evm::state::state_root(&db).0);
            if computed != evm_header.state_root {
                return Err(PruningImportError::ImportedEvmStateRootMismatch(pruning_point, computed, evm_header.state_root));
            }
        }

        // (3) Persist the rows and pin the finalized EVM head to the pruning point.
        let mut batch = WriteBatch::default();
        self.evm_header_store.insert_batch(&mut batch, pruning_point, evm_header).unwrap();
        self.evm_state_store.insert_batch(&mut batch, pruning_point, snapshot).unwrap();
        {
            let mut heads_write = self.evm_heads_store.write();
            let prev = heads_write.get().ok();
            let latest = prev.as_ref().map(|h| h.latest).unwrap_or(pruning_point);
            let safe = prev.as_ref().map(|h| h.safe).unwrap_or(pruning_point);
            let heads = kaspa_consensus_core::evm::CanonicalEvmHeads { latest, safe, finalized: pruning_point };
            heads_write.set_batch(&mut batch, heads).unwrap();
        }
        self.db.write(batch).unwrap();
        Ok(())
    }

    /// kaspa-pq ADR-0022 (serving side): the pruning point's EVM execution header +
    /// state snapshot, for a peer to stream during another node's headers-proof IBD.
    /// `None` if the overlay/EVM rows are absent (pre-activation or not yet computed).
    pub fn pruning_point_evm_state(
        &self,
        pruning_point: BlockHash,
    ) -> Option<(kaspa_consensus_core::evm::EvmExecutionHeader, kaspa_consensus_core::evm::EvmStateSnapshot)> {
        let header = self.evm_header_store.get(pruning_point).ok()?;
        let snapshot = self.evm_state_store.get(pruning_point).ok()?;
        Some((header, snapshot))
    }

    /// kaspa-pq ADR-0022: import the pruning point's DNS/PoS-v2 overlay snapshot during
    /// headers-proof IBD. Persists the bond set (so `initial_active_bond_view` and the
    /// reward path read it), the pruning point's cumulative reserve balance (read by the
    /// first post-pruning finalizing block's §F drip), and the whole snapshot in the
    /// `pruning_overlay_snapshot_store` — which `selected_chain_overlay_window` consults
    /// for the below-pruning-point window (the selected-chain walk cannot traverse below
    /// the pruning point). Verification is trustless and automatic: the first post-pruning
    /// block's existing coinbase/overlay `c == v` re-derives this state and checks it
    /// against the committed `overlay_commitment_root`; a wrong snapshot disqualifies that
    /// block and the (staging) IBD is discarded.
    pub fn import_pruning_point_overlay_snapshot(
        &self,
        pruning_point: BlockHash,
        snapshot: OverlaySnapshot,
    ) -> PruningImportResult<()> {
        if self.dns_params.is_none() {
            return Ok(()); // overlay dormant — the snapshot is empty and nothing reads it
        }
        info!(
            "Importing the overlay snapshot of the pruning point {} ({} bonds, {} window blocks, reserve {})",
            pruning_point,
            snapshot.bonds.len(),
            snapshot.window.len(),
            snapshot.reserve_balance
        );
        let mut batch = WriteBatch::default();
        {
            let mut bonds_write = self.stake_bonds_store.write();
            for rec in &snapshot.bonds {
                bonds_write.insert_batch(&mut batch, rec.bond_outpoint, std::sync::Arc::new(rec.clone())).unwrap();
            }
        }
        if snapshot.reserve_balance > 0 {
            self.reserve_balance_store.insert_batch(&mut batch, pruning_point, snapshot.reserve_balance).unwrap();
        }
        self.pruning_overlay_snapshot_store
            .write()
            .set_batch(&mut batch, PruningPointOverlaySnapshot { pruning_point, snapshot })
            .unwrap();
        self.db.write(batch).unwrap();
        Ok(())
    }

    /// kaspa-pq ADR-0022 (serving side): the persisted pruning-point overlay snapshot, for
    /// a peer to stream during another node's headers-proof IBD. `None` if the overlay is
    /// dormant or no snapshot has been captured yet (captured at pruning-advance).
    pub fn pruning_point_overlay_snapshot(&self) -> Option<PruningPointOverlaySnapshot> {
        self.pruning_overlay_snapshot_store.read().get().ok()
    }

    /// kaspa-pq ADR-0022: reconstruct the bond set as-of `pp_daa` from the never-pruned
    /// `stake_bonds_store`. A bond belongs to the as-of-pp set iff it was created
    /// (`created_daa_score`) at/below `pp_daa`; mutations stamped after `pp_daa`
    /// (slash / unbond) did not apply yet, so they are nulled. The `status` field is
    /// left as-is — `compute_overlay_snapshot` normalizes it via `effective_bond_status`
    /// at the anchor. Exact (records are never deleted, only revert-of-Insert), O(bondset).
    fn bonds_as_of(&self, pp_daa: u64) -> Vec<StakeBondRecord> {
        self.stake_bonds_store
            .read()
            .iterator()
            .filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone()))
            .filter(|rec| rec.created_daa_score <= pp_daa)
            .map(|mut rec| {
                if rec.slashed_at_daa_score.is_some_and(|d| d > pp_daa) {
                    rec.slashed_at_daa_score = None;
                }
                if rec.unbond_request_daa_score.is_some_and(|d| d > pp_daa) {
                    rec.unbond_request_daa_score = None;
                }
                rec
            })
            .collect()
    }

    /// kaspa-pq ADR-0022: capture the overlay snapshot as-of `pruning_point` into the
    /// persisted store, for serving + the below-pruning-point window consult. MUST be
    /// called BEFORE pruning deletes the below-pruning-point overlay rows (the window walk
    /// reads them). The reconstructed as-of-pp bond view + the still-present per-block
    /// rows reproduce exactly what a node computed when it validated the pruning point's
    /// child (so the first post-pruning block's `c == v` on an importer matches).
    pub fn capture_pruning_point_overlay_snapshot(&self, pruning_point: BlockHash) {
        if self.dns_params.is_none() {
            return;
        }
        let pp_daa = self.headers_store.get_daa_score(pruning_point).unwrap();
        let view = ActiveBondView::from_records(self.bonds_as_of(pp_daa).into_iter().map(|r| (r.bond_outpoint, r)));
        let snapshot = self.compute_overlay_snapshot(pruning_point, &view);
        let mut batch = WriteBatch::default();
        self.pruning_overlay_snapshot_store
            .write()
            .set_batch(&mut batch, PruningPointOverlaySnapshot { pruning_point, snapshot })
            .unwrap();
        self.db.write(batch).unwrap();
    }

    pub fn are_pruning_points_violating_finality(&self, pp_list: PruningPointsList) -> bool {
        // Ideally we would want to check if the last known pruning point has the finality point
        // in its chain, but in some cases it's impossible: let `lkp` be the last known pruning
        // point from the list, and `fup` be the first unknown pruning point (the one following `lkp`).
        // fup.blue_score - lkp.blue_score ≈ finality_depth (±k), so it's possible for `lkp` not to
        // have the finality point in its past. So we have no choice but to check if `lkp`
        // has `finality_point.finality_point` in its chain (in the worst case `fup` is one block
        // above the current finality point, and in this case `lkp` will be a few blocks above the
        // finality_point.finality_point), meaning this function can only detect finality violations
        // in depth of 2*finality_depth, and can give false negatives for smaller finality violations.
        let current_pp = self.pruning_point_store.read().pruning_point().unwrap();
        let vf = self.virtual_finality_point(&self.lkg_virtual_state.load().ghostdag_data, current_pp);
        let vff = self.depth_manager.calc_finality_point(&self.ghostdag_store.get_data(vf).unwrap(), current_pp);

        let last_known_pp = pp_list.iter().rev().find(|pp| match self.statuses_store.read().get(pp.hash).optional().unwrap() {
            Some(status) => status.is_valid(),
            None => false,
        });

        if let Some(last_known_pp) = last_known_pp {
            !self.reachability_service.is_chain_ancestor_of(vff, last_known_pp.hash)
        } else {
            // If no pruning point is known, there's definitely a finality violation
            // (normally at least genesis should be known).
            true
        }
    }

    /// Executes `op` within the thread pool associated with this processor.
    pub fn install<OP, R>(&self, op: OP) -> R
    where
        OP: FnOnce() -> R + Send,
        R: Send,
    {
        self.thread_pool.install(op)
    }
}

enum MergesetIncreaseResult {
    Accepted { increase_size: u64 },
    Rejected { new_candidate: BlockHash },
}
