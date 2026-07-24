pub mod cache_policy_builder;
pub mod ctl;
pub mod factory;
pub mod services;
pub mod storage;
pub mod test_consensus;

mod utxo_set_override;

use crate::{
    config::Config,
    errors::{BlockProcessResult, RuleError},
    model::{
        services::reachability::ReachabilityService,
        stores::{
            DB,
            acceptance_data::AcceptanceDataStoreReader,
            block_transactions::BlockTransactionsStoreReader,
            dns_state::DnsStateStoreReader,
            ghostdag::{GhostdagData, GhostdagStoreReader},
            headers::{CompactHeaderData, HeaderStoreReader},
            headers_selected_tip::HeadersSelectedTipStoreReader,
            past_pruning_points::PastPruningPointsStoreReader,
            pruning::PruningStoreReader,
            relations::RelationsStoreReader,
            selected_chain::SelectedChainStore,
            stake_bonds::StakeBondsStoreReader,
            statuses::StatusesStoreReader,
            tips::{TipsStore, TipsStoreReader},
            utxo_set::{UtxoSetStore, UtxoSetStoreReader},
            virtual_state::VirtualState,
        },
    },
    pipeline::{
        ProcessingCounters,
        body_processor::BlockBodyProcessor,
        deps_manager::{BlockProcessingMessage, BlockResultSender, BlockTask, VirtualStateProcessingMessage},
        header_processor::HeaderProcessor,
        pruning_processor::processor::{PruningProcessingMessage, PruningProcessor},
        virtual_processor::{VirtualStateProcessor, errors::PruningImportResult},
    },
    processes::{
        ghostdag::ordering::SortableBlock,
        window::{WindowManager, WindowType},
    },
};
use kaspa_consensus_core::{
    BlockHashSet, BlueWorkType, ChainPath, HashMapCustomHasher,
    acceptance_data::{AcceptanceData, MergesetBlockAcceptanceData},
    api::{
        BlockValidationFutures, ConsensusApi, ConsensusStats,
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
        stats::BlockCount,
    },
    block::{
        Block, BlockTemplate, TemplateBuildMode, TemplateTransactionSelector, TemplateTransactionSelectorFactory, VirtualStateApproxId,
    },
    blockhash::BlockHashExtensions,
    blockstatus::BlockStatus,
    coinbase::MinerData,
    daa_score_timestamp::DaaScoreTimestamp,
    dns_finality::{
        ActiveValidatorSet, AttestationQualityDeficit, CanonicalLaggedEpochAnchor, DnsConfirmation,
        MandatoryAttestationContributionKey, MandatoryAttestationDeficit, MandatoryAttestationValidator, StakeBondPage,
        StakeBondQuery, StakeBondRecord, ValidatorAttestationTarget, ValidatorRecord, dns_confirmation_from_state,
        epoch_meets_quality_floor, is_bond_active_at, paginate_stake_bonds, ready_epoch_from_tip_blue_score,
        required_stake_for_quality_floor, stake_attestation_message,
    },
    errors::{
        coinbase::CoinbaseResult,
        consensus::{ConsensusError, ConsensusResult},
        difficulty::DifficultyError,
        pruning::PruningImportError,
        tx::TxResult,
    },
    header::Header,
    mass::{ContextualMasses, NonContextualMasses},
    merkle::calc_hash_merkle_root,
    mining_rules::MiningRules,
    muhash::MuHashExtensions,
    network::NetworkType,
    pruning::{PruningPointProof, PruningPointTrustedData, PruningPointsList, PruningProofMetadata},
    trusted::{ExternalGhostdagData, TrustedBlock},
    tx::{
        MutableTransaction, Transaction, TransactionId, TransactionIndexType, TransactionOutpoint, TransactionQueryResult,
        TransactionType, UtxoEntry,
    },
};
use kaspa_consensus_notify::root::ConsensusNotificationRoot;

use crossbeam_channel::{
    Receiver as CrossbeamReceiver, Sender as CrossbeamSender, bounded as bounded_crossbeam, unbounded as unbounded_crossbeam,
};
use itertools::Itertools;
use kaspa_consensusmanager::{SessionLock, SessionReadGuard};

use kaspa_consensus_core::BlockHash;
use kaspa_core::{info, warn};
use kaspa_database::prelude::StoreResultExt;
use kaspa_muhash::MuHash;
use kaspa_txscript::caches::TxScriptCacheCounters;
use kaspa_utils::arc::ArcExtensions;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rocksdb::WriteBatch;

use std::{
    cmp,
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet, VecDeque},
    future::Future,
    iter::once,
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};
use tokio::sync::oneshot;

use self::{services::ConsensusServices, storage::ConsensusStorage};

use crate::model::stores::selected_chain::SelectedChainStoreReader;

pub struct Consensus {
    // DB
    db: Arc<DB>,

    // Channels
    block_sender: CrossbeamSender<BlockProcessingMessage>,

    // Processors
    pub(super) header_processor: Arc<HeaderProcessor>,
    pub(super) body_processor: Arc<BlockBodyProcessor>,
    pub(super) virtual_processor: Arc<VirtualStateProcessor>,
    pub(super) pruning_processor: Arc<PruningProcessor>,

    // Storage
    pub(super) storage: Arc<ConsensusStorage>,

    // Services and managers
    pub(super) services: Arc<ConsensusServices>,

    // Pruning lock
    pruning_lock: SessionLock,

    // Notification management
    notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    counters: Arc<ProcessingCounters>,

    // Config
    config: Arc<Config>,

    // Other
    creation_timestamp: u64,

    // Signals
    is_consensus_exiting: Arc<AtomicBool>,
}

impl Deref for Consensus {
    type Target = ConsensusStorage;

    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl Consensus {
    /// The raw database handle. Test-support: used by integration tests that need
    /// to write a batch directly to reach a store state (e.g. simulating a pruned
    /// anchor) the public API does not otherwise produce.
    #[cfg(test)]
    pub fn db(&self) -> Arc<DB> {
        self.db.clone()
    }

    /// One code-GC pass. Returns the number of bytecode entries deleted.
    ///
    /// The GC "epoch" is derived from wall-clock time divided by the retention
    /// interval, so quarantine ages advance at the rate passes actually run rather
    /// than at whatever rate a counter happens to be incremented. A restarted node
    /// therefore resumes the same schedule instead of resetting every sentence.
    fn run_evm_code_gc(&self, now_ms: u64) -> u64 {
        use crate::processes::evm::code_gc::{CodeGcStores, DEFAULT_QUARANTINE_EPOCHS};

        let interval_ms = self.config.evm_retention_policy.interval_ms.max(1);
        let epoch = now_ms / interval_ms;
        let stores = CodeGcStores {
            db: self.db.clone(),
            code: self.storage.evm_code_store.clone(),
            quarantine: self.storage.evm_code_quarantine_store.clone(),
            flat: self.storage.evm_flat_account_store.clone(),
            diffs: self.storage.evm_state_diff_store.clone(),
            anchors_v1: self.storage.evm_state_checkpoint_store.clone(),
            anchors_v2: self.storage.evm_state_checkpoint_v2_store.clone(),
            legacy_snapshots: self.storage.evm_state_store.clone(),
        };
        match stores.run(epoch, DEFAULT_QUARANTINE_EPOCHS) {
            Ok(r) if r.aborted => {
                warn!("[evm-retention] code GC skipped: a mark root could not be read completely (nothing was deleted)");
                0
            }
            Ok(r) => {
                if r.deleted > 0 || r.quarantined > 0 || r.released > 0 {
                    info!(
                        "[evm-retention] code GC: {} live, {} quarantined, {} released, {} deleted",
                        r.live, r.quarantined, r.released, r.deleted
                    );
                }
                r.deleted
            }
            Err(e) => {
                warn!("[evm-retention] code GC failed: {e}");
                0
            }
        }
    }

    /// Observed EVM blocks per second on the selected chain.
    ///
    /// Retention windows are expressed in time and have to be converted back into
    /// EVM-block distances. Measuring the rate instead of assuming one is the
    /// reason the setting survives a BPS change: 2048 blocks meant 34 minutes at
    /// 1 BPS and 3.4 at 10, and nothing in the code noticed the difference.
    ///
    /// Derived from the header timestamps of the EVM head and the oldest anchored
    /// block still retained. Falls back to 1.0 when there is not enough history to
    /// measure — a conservative rate keeps MORE data than the operator asked for,
    /// which is the safe direction for a wrong guess.
    fn observed_evm_blocks_per_second(&self, head_evm_number: u64) -> f64 {
        use crate::model::stores::evm::EvmNumberStoreReader;
        use crate::model::stores::headers::HeaderStoreReader;

        const SAMPLE_BLOCKS: u64 = 4_096;
        if head_evm_number < SAMPLE_BLOCKS {
            return 1.0;
        }
        let older_number = head_evm_number - SAMPLE_BLOCKS;
        let (Some(head_block), Some(older_block)) = (
            self.storage.evm_number_store.get(head_evm_number).ok().flatten(),
            self.storage.evm_number_store.get(older_number).ok().flatten(),
        ) else {
            return 1.0;
        };
        let (Ok(head_ts), Ok(older_ts)) =
            (self.storage.headers_store.get_timestamp(head_block), self.storage.headers_store.get_timestamp(older_block))
        else {
            return 1.0;
        };
        // Guard the head timestamp being at or behind the older one: block
        // timestamps are producer-supplied and only loosely monotone.
        let elapsed_ms = head_ts.saturating_sub(older_ts);
        if elapsed_ms == 0 {
            return 1.0;
        }
        (SAMPLE_BLOCKS as f64) / (elapsed_ms as f64 / 1000.0)
    }

    pub fn new(
        db: Arc<DB>,
        config: Arc<Config>,
        pruning_lock: SessionLock,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
        tx_script_cache_counters: Arc<TxScriptCacheCounters>,
        creation_timestamp: u64,
        mining_rules: Arc<MiningRules>,
    ) -> Self {
        let params = &config.params;
        let perf_params = &config.perf;
        let is_consensus_exiting: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

        //
        // Storage layer
        //

        let storage = ConsensusStorage::new(db.clone(), config.clone());

        //
        // Services and managers
        //

        let services = ConsensusServices::new(
            db.clone(),
            storage.clone(),
            config.clone(),
            tx_script_cache_counters,
            is_consensus_exiting.clone(),
        );

        //
        // Processor channels
        //

        let (sender, receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
        let (body_sender, body_receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
        let (virtual_sender, virtual_receiver): (
            CrossbeamSender<VirtualStateProcessingMessage>,
            CrossbeamReceiver<VirtualStateProcessingMessage>,
        ) = unbounded_crossbeam();
        let (pruning_sender, pruning_receiver): (
            CrossbeamSender<PruningProcessingMessage>,
            CrossbeamReceiver<PruningProcessingMessage>,
        ) = bounded_crossbeam(2);

        //
        // Thread-pools
        //

        // Pool for header and body processors
        let block_processors_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(perf_params.block_processors_num_threads)
                .thread_name(|i| format!("block-pool-{i}"))
                .build()
                .unwrap(),
        );
        // We need a dedicated thread-pool for the virtual processor to avoid possible deadlocks probably caused by the
        // combined usage of `par_iter` (in virtual processor) and `rayon::spawn` (in header/body processors).
        // See for instance https://github.com/rayon-rs/rayon/issues/690
        let virtual_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(perf_params.virtual_processor_num_threads)
                .thread_name(|i| format!("virtual-pool-{i}"))
                .build()
                .unwrap(),
        );

        //
        // Pipeline processors
        //

        let header_processor = Arc::new(HeaderProcessor::new(
            receiver,
            body_sender,
            block_processors_pool.clone(),
            params,
            db.clone(),
            &storage,
            &services,
            pruning_lock.clone(),
            counters.clone(),
        ));

        let body_processor = Arc::new(BlockBodyProcessor::new(
            body_receiver,
            virtual_sender,
            block_processors_pool,
            params,
            db.clone(),
            &storage,
            &services,
            pruning_lock.clone(),
            notification_root.clone(),
            counters.clone(),
        ));

        let virtual_processor = Arc::new(VirtualStateProcessor::new(
            virtual_receiver,
            pruning_sender,
            pruning_receiver.clone(),
            virtual_pool,
            params,
            db.clone(),
            &storage,
            &services,
            pruning_lock.clone(),
            notification_root.clone(),
            counters.clone(),
            mining_rules,
            config.evm_history_mode,         // §12: gate the archive diff/checkpoint writer
            config.evm_checkpoint_policy,    // §12.3 v2: anchor cadence + retention bound
            config.evm_retention_policy,     // segment retention (also gates the write path)
            config.evm_shadow_state_backend, // C-01 S4: node-local shadow dual-write + differential
            config.evm_flat_authoritative,   // C-01 S9: flat-authoritative executor seed
            config.evm_retire_206,           // C-01 S9b: stop persisting the per-block 206 snapshot
        ));

        let pruning_processor = Arc::new(PruningProcessor::new(
            pruning_receiver,
            db.clone(),
            &storage,
            &services,
            // kaspa-pq ADR-0022: the pruning processor captures the overlay snapshot as-of the
            // advancing pruning point (before deleting below-pp rows) via the SAME compute path
            // the virtual processor validates with, so a served snapshot matches a verifier's c==v.
            virtual_processor.clone(),
            pruning_lock.clone(),
            config.clone(),
            is_consensus_exiting.clone(),
        ));

        // Ensure the relations stores are initialized
        header_processor.init();
        // Ensure that some pruning point is registered
        virtual_processor.init();

        // Ensure that genesis was processed
        if config.process_genesis {
            header_processor.process_genesis();
            body_processor.process_genesis();
            virtual_processor.process_genesis();
        } else if let Ok(stored_genesis) = storage.past_pruning_points_store.get(0) {
            // kaspa-pq Phase 3 re-genesis guard (ADR-0007): an existing consensus DB permanently
            // records the genesis it was built from at past_pruning_points[0] (written by
            // `process_genesis`, never pruned — it is the pruning-proof anchor). If it does not match
            // the configured genesis, this data directory belongs to a DIFFERENT chain — e.g. the
            // pre-Phase-3 Argon2id chain, whose genesis hash differs after the relaunch-marker bump.
            // Refuse to silently resume it (which would graft algo_id=3 blocks onto an algo_id=2
            // history and split from freshly-genesised nodes). The operator must wipe the data
            // directory to re-genesis. Mirrors the invariant the pruning processor asserts at runtime.
            assert_eq!(
                stored_genesis, config.genesis.hash,
                "consensus DB genesis {stored_genesis} does not match the configured genesis {} — this \
                 data directory belongs to a different chain; wipe it to re-genesis onto the new chain",
                config.genesis.hash
            );
        }

        let this = Self {
            db,
            block_sender: sender,
            header_processor,
            body_processor,
            virtual_processor,
            pruning_processor,
            storage,
            services,
            pruning_lock,
            notification_root,
            counters,
            config,
            creation_timestamp,
            is_consensus_exiting,
        };

        // Run database upgrades if any
        this.run_database_upgrades();

        this
    }

    /// A procedure for calling database upgrades which are self-contained (i.e., do not require knowing the DB version)
    fn run_database_upgrades(&self) {
        // Upgrade to initialize the new retention root field correctly
        self.retention_root_database_upgrade();
        self.consensus_transitional_flags_upgrade();
        // C-01 migration: legacy databases can have authoritative full snapshots in 206 but no
        // flat/code backend. Recover it before shadow validation or irreversible 206 reclamation.
        self.evm_legacy_state_backend_backfill();
        // C-01 S9b-prune: one-shot bulk reclamation of the legacy 206 snapshot store (opt-in, gated).
        self.evm_legacy_206_bulk_prune();
        // Self-heal: if the pruning point lost its §12 anchor (a node that ran a
        // pre-fix binary with 206 retired + recent history), rebuild it by the
        // inverse walk so this node can serve pruned IBD again. No-op once present.
        self.evm_pruning_point_anchor_backfill();
    }

    /// Rebuild the pruning point's §12 anchor if it is missing.
    ///
    /// A node that ran a binary predating the pruning-point anchor fix, with 206
    /// retired and `recent` history, has no anchor at or below its pruning point.
    /// The forward reconstruction then produces the EMPTY state root and the node
    /// cannot serve pruned IBD — the live-net serveability break. There is nothing
    /// wrong with the node's own state; only its ability to serve is lost, and the
    /// material to rebuild the anchor is still on disk (the flat head plus the
    /// retained (pp, tip] diffs), so this recovers it without a resync.
    ///
    /// Self-healing rather than flag-gated: it is a correctness repair, not a
    /// feature to opt into, and an operator should not need to know the failure
    /// exists to fix it. Runs at most once per node (the anchor persists), only
    /// when the precise broken condition holds, and is a cheap no-op otherwise.
    fn evm_pruning_point_anchor_backfill(&self) {
        #[cfg(not(feature = "evm"))]
        {}
        #[cfg(feature = "evm")]
        {
            use crate::model::stores::evm::{EvmHeaderStoreReader, EvmStateCheckpointV2StoreReader, EvmStateStoreReader};
            use kaspa_consensus_core::evm::EvmStateCheckpointV2;

            if self.config.evm_activation_daa_score == u64::MAX || !self.config.evm_history_mode.writes_state_history() {
                return;
            }
            let pruning_point = match self.pruning_point_store.read().pruning_point() {
                Ok(pp) => pp,
                Err(_) => return,
            };
            // Pre-activation pruning point, or an anchor already present ⇒ nothing to do.
            let Ok(evm_header) = self.storage.evm_header_store.get(pruning_point) else { return };
            if EvmStateCheckpointV2StoreReader::has(&*self.storage.evm_state_checkpoint_v2_store, pruning_point).unwrap_or(false) {
                return;
            }
            // 206[pp] still present (not retired) ⇒ the serving path already works; no
            // anchor is needed to bootstrap. Leave it.
            if self.storage.evm_state_store.get(pruning_point).is_ok() {
                return;
            }
            let Some(flat_head) = self.storage.evm_latest_state_ptr_store.read().get().ok().flatten().map(|p| p.canonical_head) else {
                return; // no flat backend ⇒ no material to walk from
            };

            info!(
                "[evm-anchor] pruning point {pruning_point} has no §12 anchor and 206 is retired — this node cannot currently                  serve pruned IBD for the EVM lane. Rebuilding the anchor by inverse walk from the flat head (bounded by the                  pruning depth; runs once)."
            );
            const REBUILD_MAX_STEPS: usize = 4_000_000;
            let snapshot = match crate::processes::evm::reconstruct_pruning_point_snapshot_verified(
                pruning_point,
                evm_header.state_root,
                flat_head,
                &self.storage.evm_flat_account_store,
                &self.storage.evm_code_store,
                &self.storage.evm_state_diff_store,
                REBUILD_MAX_STEPS,
            ) {
                Ok(snapshot) => snapshot,
                Err(e) => {
                    warn!(
                        "[evm-anchor] could not rebuild the pruning-point anchor for {pruning_point} ({e}); this node will not                          serve pruned IBD for the EVM lane. Use --evm-history-mode=archive or resync to restore it."
                    );
                    return;
                }
            };
            let checkpoint = EvmStateCheckpointV2::build(
                pruning_point,
                evm_header.evm_number,
                evm_header.state_root,
                &snapshot,
                self.config.evm_checkpoint_policy.codec,
            );
            let mut batch = rocksdb::WriteBatch::default();
            if self.storage.evm_state_checkpoint_v2_store.insert_batch(&mut batch, pruning_point, checkpoint).is_ok()
                && self.db.write(batch).is_ok()
            {
                info!(
                    "[evm-anchor] pruning-point anchor rebuilt at EVM #{} ({pruning_point}); pruned IBD serving restored",
                    evm_header.evm_number
                );
            } else {
                warn!("[evm-anchor] pruning-point anchor rebuild computed the state for {pruning_point} but failed to persist it");
            }
        }
    }

    /// Automatically migrate a legacy 206-only EVM database to the C-01 flat/code backend.
    ///
    /// Old snapshots contain bytecode inline, so they are the recovery source for prefix 222. The
    /// canonical head is root-validated before it seeds flat state, and all historical bytecode is
    /// streamed and deduplicated before an explicitly requested legacy reclamation. This method never
    /// deletes 206; any failure leaves the old authoritative representation available.
    fn evm_legacy_state_backend_backfill(&self) {
        if !self.config.evm_shadow_state_backend {
            return;
        }
        #[cfg(not(feature = "evm"))]
        warn!("[evm-backfill] --evm-shadow-state-backend requires --features evm; migration skipped.");
        #[cfg(feature = "evm")]
        {
            use crate::model::stores::evm::{
                EvmCanonicalHeadsStoreReader, EvmCodeStoreReader, EvmHeaderStoreReader, EvmStateStoreReader,
            };
            use kaspa_consensus_core::evm::EVM_EMPTY_CODE_HASH;

            let legacy = &self.storage.evm_state_store;
            match legacy.has_any() {
                Ok(true) => {}
                Ok(false) => return,
                Err(e) => {
                    warn!("[evm-backfill] cannot probe legacy 206 snapshots ({e}); keeping 206.");
                    return;
                }
            }

            let evm_head = match self.storage.evm_heads_store.read().get() {
                Ok(h) => h.latest,
                Err(e) => {
                    warn!("[evm-backfill] cannot read canonical EVM head ({e}); keeping 206.");
                    return;
                }
            };
            let head_snapshot = match legacy.get(evm_head) {
                Ok(s) => s,
                Err(e) => {
                    warn!("[evm-backfill] canonical-head 206 snapshot {evm_head} is unavailable ({e}); keeping 206.");
                    return;
                }
            };
            let committed_root = match self.storage.evm_header_store.get(evm_head) {
                Ok(h) => h.state_root,
                Err(e) => {
                    warn!("[evm-backfill] canonical-head EVM header {evm_head} is unavailable ({e}); keeping 206.");
                    return;
                }
            };
            let snapshot_root = match kaspa_evm::snapshot::seed_cachedb(&head_snapshot) {
                Ok(cdb) => kaspa_hashes::EvmH256::from_bytes(kaspa_evm::state::state_root(&cdb).0),
                Err(e) => {
                    warn!("[evm-backfill] canonical-head 206 snapshot {evm_head} is corrupt ({e}); keeping 206.");
                    return;
                }
            };
            if snapshot_root != committed_root {
                warn!("[evm-backfill] canonical-head 206 root {snapshot_root:?} != committed {committed_root:?}; keeping 206.");
                return;
            }

            // If 206 is about to disappear, recover code for contracts that existed only in an old
            // snapshot too. Chunked writes keep memory and write batches bounded; partial backfill is
            // harmless because 222 is content-addressed and 206 remains until the later safety gate.
            if self.config.evm_prune_legacy_206 {
                const CODE_BATCH_ROWS: usize = 1024;
                info!(
                    "[evm-backfill] legacy 206 reclamation requested; streaming all snapshots to recover historical bytecode. \
                     This one-shot startup migration may take a while on a large database."
                );
                let mut seen = HashSet::new();
                let mut batch = WriteBatch::default();
                let mut pending = 0usize;
                let mut snapshots = 0u64;
                let mut accounts = 0u64;
                let mut recovered = 0u64;

                for row in legacy.iter() {
                    let (_, snapshot) = match row {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("[evm-backfill] 206 scan failed after {snapshots} snapshots ({e}); keeping 206.");
                            return;
                        }
                    };
                    snapshots += 1;
                    accounts += snapshot.accounts.len() as u64;
                    if snapshots.is_multiple_of(1_000) {
                        info!(
                            "[evm-backfill] progress: scanned {snapshots} legacy snapshots / {accounts} account copies; \
                             recovered {recovered} unique bytecode entries."
                        );
                    }
                    for account in snapshot.accounts {
                        if account.code.is_empty() || account.code_hash == EVM_EMPTY_CODE_HASH || !seen.insert(account.code_hash) {
                            continue;
                        }
                        let computed = kaspa_evm::snapshot::code_hash(&account.code);
                        if computed != account.code_hash {
                            warn!(
                                "[evm-backfill] corrupt inline code: declared {:?}, computed {:?}; keeping 206.",
                                account.code_hash, computed
                            );
                            return;
                        }
                        match self.storage.evm_code_store.get(account.code_hash) {
                            Ok(Some(existing)) if existing == account.code => continue,
                            Ok(Some(_)) => {
                                warn!("[evm-backfill] code-store collision for {:?}; keeping 206.", account.code_hash);
                                return;
                            }
                            Ok(None) => {}
                            Err(e) => {
                                warn!("[evm-backfill] code-store read failed ({e}); keeping 206.");
                                return;
                            }
                        }
                        if let Err(e) = self.storage.evm_code_store.write_batch(&mut batch, account.code_hash, account.code) {
                            warn!("[evm-backfill] could not stage recovered code ({e}); keeping 206.");
                            return;
                        }
                        pending += 1;
                        recovered += 1;
                        if pending >= CODE_BATCH_ROWS {
                            if let Err(e) = self.db.write(batch) {
                                warn!("[evm-backfill] code backfill write failed ({e}); keeping 206.");
                                return;
                            }
                            batch = WriteBatch::default();
                            pending = 0;
                        }
                    }
                }
                if pending != 0
                    && let Err(e) = self.db.write(batch)
                {
                    warn!("[evm-backfill] final code backfill write failed ({e}); keeping 206.");
                    return;
                }
                info!(
                    "[evm-backfill] scanned {snapshots} legacy snapshots / {accounts} account copies; \
                     recovered {recovered} unique bytecode entries into prefix 222."
                );
            }

            // Seed/reseed the flat backend from the validated head. This writes all currently-live
            // bytecode as well, so a cheap shadow-only rollout needs no full historical scan.
            let mut batch = WriteBatch::default();
            let mut ptr = self.storage.evm_latest_state_ptr_store.write();
            if let Err(e) = crate::processes::evm::seed_flat_from_snapshot(
                &self.storage.evm_flat_account_store,
                &self.storage.evm_code_store,
                &self.storage.evm_block_state_root_store,
                &mut ptr,
                &mut batch,
                evm_head,
                committed_root,
                &head_snapshot,
            ) {
                warn!("[evm-backfill] could not stage flat-state seed ({e}); keeping 206.");
                return;
            }
            if let Err(e) = self.db.write(batch) {
                warn!("[evm-backfill] flat-state seed write failed ({e}); keeping 206.");
                return;
            }
            drop(ptr);

            let verified_root =
                crate::processes::evm::materialize_snapshot(&self.storage.evm_flat_account_store, &self.storage.evm_code_store)
                    .map_err(|e| e.to_string())
                    .and_then(|snap| kaspa_evm::snapshot::seed_cachedb(&snap).map_err(|e| e.to_string()))
                    .map(|cdb| kaspa_hashes::EvmH256::from_bytes(kaspa_evm::state::state_root(&cdb).0));
            match verified_root {
                Ok(root) if root == committed_root => {
                    info!("[evm-backfill] flat/code backend seeded and root-verified at EVM head {evm_head} ({committed_root:?}).")
                }
                Ok(root) => warn!("[evm-backfill] flat root {root:?} != committed {committed_root:?}; keeping 206."),
                Err(e) => warn!("[evm-backfill] flat verification failed ({e}); keeping 206."),
            }
        }
    }

    /// C-01 (slice S9b-prune): a ONE-SHOT, IRREVERSIBLE bulk reclamation of the legacy per-block 206
    /// EVM state-snapshot store, run at startup when `--evm-prune-legacy-206` is set. The per-block
    /// pruner (`pruning_processor`) already reclaims 206 for blocks as they fall below the pruning
    /// point; this brings forward the reclamation of the rows still above it (and, on archival nodes
    /// that never prune, all of them) rather than waiting for the pruning point to slide.
    ///
    /// SAFETY GATE: refused (warn + no-op) unless `--evm-retire-206` is EFFECTIVE — i.e. paired with
    /// `--evm-flat-authoritative` + `--evm-shadow-state-backend` (the exact condition under which the
    /// virtual processor does not demote retire-206). Under that gate the executor seeds from the
    /// validated flat/reconstruct parent (`validated_flat_parent_seed`) and a present 206 is only a
    /// redundant byte-compare oracle, so deleting every 206 row leaves the seed itself unchanged; the
    /// read paths (`get_evm_state_snapshot_of`, the IBD pruning-point export) already fall back
    /// 206 → flat-materialize → §12-reconstruct. Deleting 206 WITHOUT that gate would remove the
    /// executor's only seed source and HALT the node — hence the refusal. Node-local, consensus-neutral.
    /// After the one run the store is empty, so subsequent startups are a fast no-op.
    fn evm_legacy_206_bulk_prune(&self) {
        if !self.config.evm_prune_legacy_206 {
            return;
        }
        // Same prerequisite chain the virtual processor uses to keep `evm_retire_206` effective.
        let retire_effective =
            self.config.evm_retire_206 && self.config.evm_flat_authoritative && self.config.evm_shadow_state_backend;
        if !retire_effective {
            warn!(
                "[C-01 S9b-prune] --evm-prune-legacy-206 is set but --evm-retire-206 is not effective \
                 (it also needs --evm-flat-authoritative + --evm-shadow-state-backend). The 206 store may \
                 still be the executor seed source, so refusing the IRREVERSIBLE bulk delete. No data was touched."
            );
            return;
        }
        #[cfg(not(feature = "evm"))]
        {
            warn!(
                "[C-01 S9b-prune] --evm-prune-legacy-206 requires a kaspad built with --features evm; skipping (no EVM state on this build)."
            );
        }
        #[cfg(feature = "evm")]
        {
            use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader};

            let store = &self.storage.evm_state_store;
            // Nothing to do if the store is already empty (the steady state after the one-shot run, or a
            // node that only ever ran retired). Probe before any destructive action.
            match store.has_any() {
                Ok(false) => {
                    info!("[C-01 S9b-prune] no legacy 206 snapshot rows present; nothing to reclaim.");
                    return;
                }
                Ok(true) => {}
                Err(e) => {
                    warn!("[C-01 S9b-prune] could not probe the legacy 206 store ({e}); skipping the bulk reclamation this startup.");
                    return;
                }
            }

            // History-mode refusal: `head` keeps no §12 state history (no diff/checkpoint), so a node
            // there cannot reconstruct a non-head parent. Under effective retire-206 such a reorg already
            // HALTs (no 206 fallback), and keeping the legacy 206 rows is the ONLY way to roll retire-206
            // back to a working 206 seed. Deleting them on a `head` node removes that last recovery — refuse.
            if !self.config.evm_history_mode.writes_state_history() {
                warn!(
                    "[C-01 S9b-prune] --evm-history-mode=head keeps no §12 state history, so the legacy 206 rows are the only \
                     remaining way to recover the executor seed if retire-206 must be rolled back. Refusing the IRREVERSIBLE \
                     bulk delete on a head-mode node; switch to --evm-history-mode=recent/archive to prune 206. 206 left in place."
                );
                return;
            }

            // CRITICAL pre-flight: removing 206 removes the recovery net. Under EFFECTIVE retire-206 an
            // unavailable flat parent seed HALTs the node (it does NOT fall back to 206), so deleting 206
            // before the flat backend is genuinely current + faithful would brick the node IRREVERSIBLY.
            // Verify, from reliably-persisted stores only (NOT the lkg virtual-state cache, which is
            // `default()` until the worker runs), that the flat store materializes the canonical EVM head
            // and — the gold-standard check before an irreversible delete — that the on-disk flat ACCOUNT
            // ROWS actually keccak-MPT-hash to that head's committed `state_root` (not merely that the
            // stored pointer claims so). This fails (⇒ refuse) when the flat store was never warmed up
            // (pointer absent), is stale (head mismatch), or is corrupt/incomplete (recomputed root
            // mismatch) — exactly the cases where 206 must be kept as the seed source.
            let flat_ptr = match self.storage.evm_latest_state_ptr_store.read().get() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    warn!(
                        "[C-01 S9b-prune] the flat state pointer is absent — the flat backend was never initialized on this node. Refusing the IRREVERSIBLE 206 delete; 206 stays as the executor seed. Warm up --evm-shadow-state-backend + --evm-flat-authoritative first."
                    );
                    return;
                }
                Err(e) => {
                    warn!(
                        "[C-01 S9b-prune] flat state pointer read failed ({e}); refusing the bulk delete (cannot prove the flat backend is current). 206 left in place."
                    );
                    return;
                }
            };
            let evm_head = match self.storage.evm_heads_store.read().get() {
                Ok(h) => h.latest,
                Err(e) => {
                    warn!(
                        "[C-01 S9b-prune] canonical EVM head read failed ({e}) while 206 rows exist; refusing the bulk delete. 206 left in place."
                    );
                    return;
                }
            };
            if flat_ptr.canonical_head != evm_head {
                warn!(
                    "[C-01 S9b-prune] the flat backend is stale — it materializes block {} but the canonical EVM head is {}. \
                     Refusing the IRREVERSIBLE 206 delete so the seed source is preserved. Run with --evm-shadow-state-backend \
                     + --evm-flat-authoritative until the flat store converges to the head, then restart with --evm-prune-legacy-206.",
                    flat_ptr.canonical_head, evm_head
                );
                return;
            }
            let committed_root = match self.storage.evm_header_store.get(evm_head) {
                Ok(h) => h.state_root,
                Err(e) => {
                    warn!(
                        "[C-01 S9b-prune] could not read the committed EVM header for the canonical head {evm_head} ({e}); refusing the bulk delete (cannot verify the flat backend). 206 left in place."
                    );
                    return;
                }
            };
            // Re-derive the flat state root from the actual on-disk account rows (materialize 234 + code,
            // then keccak-MPT) and require it to equal the committed head root. Catches silent flat-store
            // corruption that a trusted pointer field would miss. O(state) — one-shot, on the startup path.
            let recomputed_root = match crate::processes::evm::materialize_snapshot(
                &self.storage.evm_flat_account_store,
                &self.storage.evm_code_store,
            )
            .map_err(|e| e.to_string())
            .and_then(|snap| kaspa_evm::snapshot::seed_cachedb(&snap).map_err(|e| e.to_string()))
            .map(|cdb| kaspa_hashes::EvmH256::from_bytes(kaspa_evm::state::state_root(&cdb).0))
            {
                Ok(r) => r,
                Err(e) => {
                    warn!(
                        "[C-01 S9b-prune] could not recompute the flat state root ({e}); refusing the bulk delete (cannot verify the flat backend is faithful). 206 left in place."
                    );
                    return;
                }
            };
            if recomputed_root != committed_root {
                warn!(
                    "[C-01 S9b-prune] the flat backend is NOT faithful at the EVM head {evm_head}: its account rows hash to {recomputed_root:?} \
                     but the committed state_root is {committed_root:?}. Refusing the IRREVERSIBLE 206 delete (206 is the last faithful copy). \
                     Restore/re-shadow the flat backend before pruning. 206 left in place."
                );
                return;
            }

            // Verified: the flat store is the authoritative, current, faithful post-state at the EVM head, so
            // every 206 row is now pure redundancy (its only remaining use was a byte-compare oracle).
            warn!(
                "[C-01 S9b-prune] --evm-prune-legacy-206: flat backend verified current at EVM head {evm_head}; IRREVERSIBLY \
                 bulk-deleting the legacy per-block 206 EVM state-snapshot store and compacting the reclaimed range. This may \
                 take a while on a large store; the flat backend remains the authoritative post-state (seed + reads unaffected)."
            );
            match store.bulk_delete_all_and_compact() {
                Ok(()) => info!("[C-01 S9b-prune] legacy 206 snapshot store reclaimed; space returned to the OS after compaction."),
                // A failure here leaves 206 present (delete_range is a single direct write); the node keeps
                // running on the flat seed regardless. Surface it loudly; do not abort startup.
                Err(e) => warn!(
                    "[C-01 S9b-prune] bulk reclamation of the legacy 206 store FAILED: {e}; 206 left in place (harmless — flat backend is authoritative). Retry later."
                ),
            }
        }
    }

    fn retention_root_database_upgrade(&self) {
        let mut pruning_point_store = self.pruning_point_store.write();
        if pruning_point_store.retention_period_root().optional().unwrap().is_none() {
            let mut batch = rocksdb::WriteBatch::default();
            if self.config.is_archival {
                // The retention checkpoint is what was previously known as history root
                let retention_checkpoint = pruning_point_store.retention_checkpoint().unwrap();
                pruning_point_store.set_retention_period_root(&mut batch, retention_checkpoint).unwrap();
            } else {
                // For non-archival nodes the retention root was the pruning point
                let pruning_point = pruning_point_store.pruning_point().unwrap();
                pruning_point_store.set_retention_period_root(&mut batch, pruning_point).unwrap();
            }
            self.db.write(batch).unwrap();
        }
    }

    fn consensus_transitional_flags_upgrade(&self) {
        // Write the defaults to the internal storage so they will remain in cache
        // *For a new staging consensus these flags will be updated again explicitly*
        let mut batch = rocksdb::WriteBatch::default();
        let mut pruning_meta_write = self.storage.pruning_meta_stores.write();
        if pruning_meta_write.is_anticone_fully_synced() {
            pruning_meta_write.set_body_missing_anticone(&mut batch, vec![]).unwrap();
        }
        if pruning_meta_write.pruning_utxoset_stable_flag() {
            pruning_meta_write.set_pruning_utxoset_stable_flag(&mut batch, true).unwrap();
        }
        self.db.write(batch).unwrap();
    }

    pub fn run_processors(&self) -> Vec<JoinHandle<()>> {
        // Spawn the asynchronous processors.
        let header_processor = self.header_processor.clone();
        let body_processor = self.body_processor.clone();
        let virtual_processor = self.virtual_processor.clone();
        let pruning_processor = self.pruning_processor.clone();

        // QR startup hardening: complete any interrupted prune BEFORE the virtual processor resolves.
        // Pruning recovery was previously gated behind the pruning worker's first Process message, whose
        // sole producer is resolve_virtual -- but a half-pruned DB (reachability rows deleted while still
        // referenced by finality_point/body_tips/sink) makes resolve_virtual panic first, so recovery could
        // never run (bootstrap deadlock -> crash-loop). Running it here synchronously, before the virtual
        // processor starts, lets the node self-heal. Idempotent (the worker's own check is then a no-op)
        // and consensus-neutral (only changes WHEN the existing recovery runs).
        if !pruning_processor.recover_pruning_workflows_if_needed() {
            info!(
                "Startup pruning recovery deferred (consensus transitional or catching up); the pruning worker will retry on its first message"
            );
        }

        vec![
            thread::Builder::new().name("header-processor".to_string()).spawn(move || header_processor.worker()).unwrap(),
            thread::Builder::new().name("body-processor".to_string()).spawn(move || body_processor.worker()).unwrap(),
            thread::Builder::new().name("virtual-processor".to_string()).spawn(move || virtual_processor.worker()).unwrap(),
            thread::Builder::new().name("pruning-processor".to_string()).spawn(move || pruning_processor.worker()).unwrap(),
        ]
    }

    /// Acquires a consensus session, blocking data-pruning from occurring until released
    pub fn acquire_session(&self) -> SessionReadGuard<'_> {
        self.pruning_lock.blocking_read()
    }

    fn validate_and_insert_block_impl(
        &self,
        task: BlockTask,
    ) -> (
        impl Future<Output = BlockProcessResult<BlockStatus>> + 'static,
        impl Future<Output = BlockProcessResult<BlockStatus>> + 'static,
    ) {
        let (btx, brx): (BlockResultSender, _) = oneshot::channel();
        let (vtx, vrx): (BlockResultSender, _) = oneshot::channel();
        self.block_sender.send(BlockProcessingMessage::Process(task, btx, vtx)).unwrap();
        self.counters.blocks_submitted.fetch_add(1, Ordering::Relaxed);
        (async { brx.await.unwrap() }, async { vrx.await.unwrap() })
    }

    pub fn body_tips(&self) -> BlockHashSet {
        self.body_tips_store.read().get().unwrap().read().clone()
    }

    pub fn block_status(&self, hash: BlockHash) -> BlockStatus {
        self.statuses_store.read().get(hash).unwrap()
    }

    pub fn session_lock(&self) -> SessionLock {
        self.pruning_lock.clone()
    }

    pub fn notification_root(&self) -> Arc<ConsensusNotificationRoot> {
        self.notification_root.clone()
    }

    pub fn processing_counters(&self) -> &Arc<ProcessingCounters> {
        &self.counters
    }

    pub fn signal_exit(&self) {
        self.is_consensus_exiting.store(true, Ordering::Relaxed);
        self.block_sender.send(BlockProcessingMessage::Exit).unwrap();
    }

    pub fn shutdown(&self, wait_handles: Vec<JoinHandle<()>>) {
        self.signal_exit();
        // Wait for async consensus processors to exit
        for handle in wait_handles {
            handle.join().unwrap();
        }
    }

    /// Validates that a valid block *header* exists for `hash`
    fn validate_block_exists(&self, hash: BlockHash) -> Result<(), ConsensusError> {
        if match self.statuses_store.read().get(hash).optional().unwrap() {
            Some(status) => status.is_valid(),
            None => false,
        } {
            Ok(())
        } else {
            Err(ConsensusError::HeaderNotFound(hash))
        }
    }

    fn estimate_network_hashes_per_second_impl(&self, ghostdag_data: &GhostdagData, window_size: usize) -> ConsensusResult<u64> {
        let window = match self.services.window_manager.block_window(ghostdag_data, WindowType::VaryingWindow(window_size)) {
            Ok(w) => w,
            Err(RuleError::InsufficientDaaWindowSize(s)) => return Err(DifficultyError::InsufficientWindowData(s).into()),
            Err(e) => panic!("unexpected error: {e}"),
        };
        Ok(self.services.window_manager.estimate_network_hashes_per_second(window)?)
    }

    fn pruning_point_compact_headers(&self) -> Vec<(BlockHash, CompactHeaderData)> {
        // PRUNE SAFETY: index is monotonic and past pruning point headers are expected permanently
        let (pruning_point, pruning_index) = self.pruning_point_store.read().pruning_point_and_index().unwrap();
        (0..pruning_index)
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .chain(once(pruning_point))
            .map(|hash| (hash, self.headers_store.get_compact_header_data(hash).unwrap()))
            .collect_vec()
    }

    /// See: intrusive_pruning_point_update implementation below for details
    pub fn intrusive_pruning_point_store_writes(
        &self,
        new_pruning_point: BlockHash,
        syncer_sink: BlockHash,
        pruning_points_to_add: VecDeque<BlockHash>,
    ) -> ConsensusResult<()> {
        let mut batch = WriteBatch::default();
        let mut pruning_point_write = self.pruning_point_store.write();
        let old_pp_index = pruning_point_write.pruning_point_index().unwrap();
        let retention_period_root = pruning_point_write.retention_period_root().unwrap();

        let new_pp_index = old_pp_index + pruning_points_to_add.len() as u64;
        pruning_point_write.set_batch(&mut batch, new_pruning_point, new_pp_index).unwrap();
        for (i, &past_pp) in pruning_points_to_add.iter().rev().enumerate() {
            self.past_pruning_points_store.insert_batch(&mut batch, old_pp_index + i as u64 + 1, past_pp).unwrap();
        }

        // For archival nodes, keep the retention root in place
        if !self.config.is_archival {
            let adjusted_retention_period_root =
                self.pruning_processor.advance_retention_period_root(retention_period_root, new_pruning_point);
            pruning_point_write.set_retention_period_root(&mut batch, adjusted_retention_period_root).unwrap();
        }

        // Update virtual state based to the new pruning point
        // Updating of the utxoset is done separately as it requires downloading the new utxoset in its entirety.
        let virtual_parents = vec![new_pruning_point];
        let virtual_state = Arc::new(VirtualState {
            parents: virtual_parents.clone(),
            ghostdag_data: self.services.ghostdag_manager.ghostdag(&virtual_parents),
            ..VirtualState::default()
        });
        self.virtual_stores.write().state.set_batch(&mut batch, virtual_state).unwrap();
        // Remove old body tips and insert pruning point as the current tip
        self.body_tips_store.write().delete_all_tips(&mut batch).unwrap();
        self.body_tips_store.write().init_batch(&mut batch, &virtual_parents).unwrap();
        // Update selected_chain
        self.selected_chain_store.write().init_with_pruning_point(&mut batch, new_pruning_point).unwrap();
        // It is important to set this flag to false together with writing the batch, in case the node crashes suddenly before syncing of new utxo starts
        self.pruning_meta_stores.write().set_pruning_utxoset_stable_flag(&mut batch, false).unwrap();
        // Store the currently bodyless anticone from the POV of the syncer, for trusted body validation at a later stage.
        let mut anticone = self.services.dag_traversal_manager.anticone(new_pruning_point, [syncer_sink].into_iter(), None)?;
        // Add the pruning point itself which is also missing a body
        anticone.push(new_pruning_point);
        self.pruning_meta_stores.write().set_body_missing_anticone(&mut batch, anticone).unwrap();
        self.db.write(batch).unwrap();
        drop(pruning_point_write);
        Ok(())
    }

    /// Verify that the new pruning point can be safely imported
    /// and return all new pruning point on path to it that needs to be updated in consensus
    fn get_and_verify_path_to_new_pruning_point(
        &self,
        new_pruning_point: BlockHash,
        syncer_sink: BlockHash,
    ) -> ConsensusResult<VecDeque<BlockHash>> {
        // Let B.sp denote the selected parent of a block B, let f be the finality depth, and let p be the pruning depth.
        // The new pruning point P can be "finalized" into consensus if:
        // 1) P satisfies P.blue_score>Nf and selected_parent(P).blue_score<=NF
        // where N is some integer (i.e. it is a valid pruning point based on score)
        let Ok(candidate_ghostdag_data) = self.get_ghostdag_data(new_pruning_point) else {
            return Err(ConsensusError::General(
                "Catchup cannot be continued since the syncer pruning point could not be confirmed to be a valid pruning point",
            ));
        };
        let Ok(selected_parent_ghostdag_data) = self.get_ghostdag_data(candidate_ghostdag_data.selected_parent) else {
            return Err(ConsensusError::General(
                "Catchup cannot be continued since the syncer pruning point could not be confirmed to be a valid pruning point",
            ));
        };
        self.services
            .pruning_point_manager
            .is_pruning_sample(
                candidate_ghostdag_data.blue_score,
                selected_parent_ghostdag_data.blue_score,
                self.config.params.finality_depth(),
            )
            .then_some(())
            .ok_or(ConsensusError::General("the alleged pruning point is not a valid pruning point, aborting catchup attempt"))?;

        // 2) There are sufficient headers built on top of it, specifically,
        // a header is validated whose blue_score is greater than P.B+p:
        let syncer_pp_bscore = self.get_header(new_pruning_point).unwrap().blue_score;
        let syncer_virtual_bscore = self.get_header(syncer_sink).unwrap().blue_score;
        if syncer_virtual_bscore < syncer_pp_bscore + self.config.pruning_depth() {
            return Err(ConsensusError::General("declared pruning point is not of sufficient depth"));
        }
        // 3) The syncer pruning point is on the selected chain from that header.
        if !self.services.reachability_service.is_chain_ancestor_of(new_pruning_point, syncer_sink) {
            return Err(ConsensusError::General("new pruning point is not in the past of syncer sink"));
        }
        info!("Setting {new_pruning_point} as the pruning point");
        // 4) The pruning points declared on headers on that path must be consistent with those already known by the node:
        let pruning_point_read = self.pruning_point_store.read();
        let old_pruning_point = pruning_point_read.pruning_point().unwrap();

        // Note that the function below also updates the pruning samples,
        // and implicitly confirms any pruning point pointed at en route to virtual is a pruning sample.
        // it is emphasized that updating pruning samples for individual blocks is not harmful
        // even if the verification ultimately does not succeed.
        let mut pruning_points_to_add =
            self.services.pruning_point_manager.pruning_points_on_path_to_syncer_sink(old_pruning_point, syncer_sink).map_err(
                |e: PruningImportError| {
                    ConsensusError::GeneralOwned(format!("pruning points en route to syncer sink do not form a valid chain: {}", e))
                },
            )?;
        // next we filter the returned list so it contains only the pruning point that must be introduced to consensus

        // Remove the excess pruning points before the old pruning point
        while let Some(past_pp) = pruning_points_to_add.pop_back() {
            if past_pp == old_pruning_point {
                break;
            }
        }
        if pruning_points_to_add.is_empty() {
            return Err(ConsensusError::General("old pruning points is inconsistent with synced headers"));
        }
        // Remove the excess pruning points beyond the new pruning_point
        while let Some(&future_pp) = pruning_points_to_add.front() {
            if future_pp == new_pruning_point {
                break;
            }
            // Here we only pop_front after checking as we want the new pruning_point to stay in the list
            pruning_points_to_add.pop_front();
        }
        if pruning_points_to_add.is_empty() {
            return Err(ConsensusError::General("new pruning point is inconsistent with synced headers"));
        }
        Ok(pruning_points_to_add)
    }

    /// kaspa-pq Phase 11 (ADR-0010): the active validator set at `pov_daa_score`,
    /// assembled from the stake-bond store (bonds active per `is_bond_active_at`).
    /// Shared by `get_active_validator_set` and `get_validator_attestation_target`
    /// so both observe an identical active set. `flatten()` drops unreadable entries
    /// defensively — a single corrupt bond must not blank out the set.
    fn dns_active_validator_records(&self, pov_daa_score: u64) -> Vec<ValidatorRecord> {
        let store = self.storage.stake_bonds_store.read();
        store
            .iterator()
            .flatten()
            .filter(|(_, record)| is_bond_active_at(record, pov_daa_score))
            .map(|(_, record)| ValidatorRecord {
                validator_id: record.validator_pubkey_hash,
                stake_amount: record.amount,
                activation_daa_score: record.activation_daa_score,
            })
            .collect()
    }

    /// kaspa-pq DNS v3: assemble the signed `ValidatorAttestationTarget` for a canonical
    /// lagged anchor — the exact `(net_id, epoch, target_hash, target_daa_score, vsc=0,
    /// bond)` digest the v3 verifier reconstructs (`collect_stake_contributions_v2`). The VSC
    /// is a fixed zero (P-1D: ADR-0017 retired the committee; not a gate, kept for domain
    /// separation). The service only signs `message`. Shared by the singular + batch signers.
    fn build_attestation_target(
        &self,
        anchor: &CanonicalLaggedEpochAnchor,
        bond_outpoint: TransactionOutpoint,
    ) -> ValidatorAttestationTarget {
        let vsc = kaspa_consensus_core::Hash64::default();
        // ADR-0009 Addendum A.3: network discriminator := the per-network genesis hash.
        let message = stake_attestation_message(
            self.config.params.genesis.hash.as_byte_slice(),
            anchor.epoch,
            anchor.anchor_hash,
            anchor.anchor_daa_score,
            vsc,
            bond_outpoint,
        );
        ValidatorAttestationTarget {
            epoch: anchor.epoch,
            target_hash: anchor.anchor_hash,
            target_daa_score: anchor.anchor_daa_score,
            validator_set_commitment: vsc,
            message,
        }
    }
}

impl ConsensusApi for Consensus {
    fn build_block_template(
        &self,
        miner_data: MinerData,
        tx_selector: Box<dyn TemplateTransactionSelector>,
        build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, RuleError> {
        self.virtual_processor.build_block_template(miner_data, tx_selector, build_mode, Default::default())
    }

    fn build_block_template_with_evm(
        &self,
        miner_data: MinerData,
        tx_selector: Box<dyn TemplateTransactionSelector>,
        build_mode: TemplateBuildMode,
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        self.virtual_processor.build_block_template(miner_data, tx_selector, build_mode, evm_template_data)
    }

    fn build_block_template_with_selector_factory(
        &self,
        miner_data: MinerData,
        tx_selector_factory: &dyn TemplateTransactionSelectorFactory,
        build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, RuleError> {
        self.virtual_processor.build_block_template_with_selector_factory(
            miner_data,
            tx_selector_factory,
            build_mode,
            Default::default(),
        )
    }

    fn build_block_template_with_evm_selector_factory(
        &self,
        miner_data: MinerData,
        tx_selector_factory: &dyn TemplateTransactionSelectorFactory,
        build_mode: TemplateBuildMode,
        evm_template_data: kaspa_consensus_core::evm::EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        self.virtual_processor.build_block_template_with_selector_factory(
            miner_data,
            tx_selector_factory,
            build_mode,
            evm_template_data,
        )
    }

    fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
        let (block_task, virtual_state_task) = self.validate_and_insert_block_impl(BlockTask::Ordinary { block });
        BlockValidationFutures { block_task: Box::pin(block_task), virtual_state_task: Box::pin(virtual_state_task) }
    }

    fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BlockValidationFutures {
        let (block_task, virtual_state_task) = self.validate_and_insert_block_impl(BlockTask::Trusted { block: tb.block });
        BlockValidationFutures { block_task: Box::pin(block_task), virtual_state_task: Box::pin(virtual_state_task) }
    }

    fn validate_mempool_transaction(&self, transaction: &mut MutableTransaction, args: &TransactionValidationArgs) -> TxResult<()> {
        self.virtual_processor.validate_mempool_transaction(transaction, args)?;
        Ok(())
    }

    fn validate_mempool_transactions_in_parallel(
        &self,
        transactions: &mut [MutableTransaction],
        args: &TransactionValidationBatchArgs,
    ) -> Vec<TxResult<()>> {
        self.virtual_processor.validate_mempool_transactions_in_parallel(transactions, args)
    }

    fn populate_mempool_transaction(&self, transaction: &mut MutableTransaction) -> TxResult<()> {
        self.virtual_processor.populate_mempool_transaction(transaction)?;
        Ok(())
    }

    fn populate_mempool_transactions_in_parallel(&self, transactions: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        self.virtual_processor.populate_mempool_transactions_in_parallel(transactions)
    }

    fn calculate_transaction_non_contextual_masses(&self, transaction: &Transaction) -> NonContextualMasses {
        self.services.mass_calculator.calc_non_contextual_masses(transaction)
    }

    fn calculate_transaction_contextual_masses(&self, transaction: &MutableTransaction) -> Option<ContextualMasses> {
        self.services.mass_calculator.calc_contextual_masses(&transaction.as_verifiable())
    }

    fn get_stats(&self) -> ConsensusStats {
        // This method is designed to return stats asap and not depend on locks which
        // might take time to acquire
        ConsensusStats {
            block_counts: self.estimate_block_count(),
            // This call acquires the tips store read lock which is expected to be fast. If this
            // turns out to be not fast enough then we should maintain an atomic integer holding this value
            num_tips: self.get_tips_len() as u64,
            virtual_stats: self.lkg_virtual_state.load().as_ref().into(),
        }
    }

    fn get_virtual_daa_score(&self) -> u64 {
        self.lkg_virtual_state.load().daa_score
    }

    fn get_virtual_bits(&self) -> u32 {
        self.lkg_virtual_state.load().bits
    }

    fn get_virtual_past_median_time(&self) -> u64 {
        self.lkg_virtual_state.load().past_median_time
    }

    fn get_virtual_merge_depth_root(&self) -> Option<BlockHash> {
        // TODO: consider saving the merge depth root as part of virtual state
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let virtual_state = self.lkg_virtual_state.load();
        let virtual_ghostdag_data = &virtual_state.ghostdag_data;
        let root = self.services.depth_manager.calc_merge_depth_root(virtual_ghostdag_data, pruning_point);
        if root.is_origin() { None } else { Some(root) }
    }

    fn get_virtual_merge_depth_blue_work_threshold(&self) -> BlueWorkType {
        // PRUNE SAFETY: merge depth root is never close to being pruned (in terms of block depth)
        self.get_virtual_merge_depth_root().map_or(BlueWorkType::ZERO, |root| self.ghostdag_store.get_blue_work(root).unwrap())
    }

    fn get_sink(&self) -> BlockHash {
        self.lkg_virtual_state.load().ghostdag_data.selected_parent
    }

    fn get_sink_timestamp(&self) -> u64 {
        self.headers_store.get_timestamp(self.get_sink()).unwrap()
    }

    fn get_sink_blue_score(&self) -> u64 {
        self.headers_store.get_blue_score(self.get_sink()).unwrap()
    }

    fn get_dns_confirmation(&self) -> Option<DnsConfirmation> {
        // kaspa-pq Phase 10 (ADR-0009): build the DNS confirmation view from the
        // current DnsState + this network's thresholds. `None` when the overlay
        // is not configured or no DnsState has been written yet.
        let dns_params = self.config.params.dns_params.as_ref()?;
        let state = self.storage.dns_state_store.read().get().ok()?;
        Some(dns_confirmation_from_state(&state, dns_params.required_work_depth, dns_params.required_stake_depth))
    }

    fn get_stake_bond(&self, bond_outpoint: TransactionOutpoint) -> Option<StakeBondRecord> {
        // kaspa-pq Phase 11 (ADR-0010): look up a stake bond by outpoint for the
        // validator service's eligibility check. `None` when the overlay is not
        // configured for this network or the bond is absent from the store.
        self.config.params.dns_params.as_ref()?;
        let record = self.storage.stake_bonds_store.read().get(&bond_outpoint).ok()?;
        Some((*record).clone())
    }

    fn get_stake_bonds(&self, query: StakeBondQuery) -> StakeBondPage {
        // kaspa-pq: paged enumeration of the StakeBonds overlay store behind the
        // GetStakeBonds RPC. The store is outpoint-keyed with no secondary owner
        // index, so owner/status filtering is a full scan; `paginate_stake_bonds`
        // applies the filters, ordering, cursor, and limit (unit-tested there).
        //
        // Cost/exposure note: like the sibling `get_mandatory_attestation_deficits`
        // / `get_attestation_quality_deficits`, this clones the whole store per call
        // (the page cap bounds the wire response, not the scan). The bond set is
        // validator-order and economically bounded on mainnet; operators should not
        // expose unauthenticated wRPC on low-`min_bond` networks, same as those ops.
        if self.config.params.dns_params.is_none() {
            return StakeBondPage::default();
        }
        // Effective status is evaluated at `pov`: the caller-pinned score if given
        // (so a status-filtered multi-page walk stays a consistent snapshot),
        // otherwise the live sink (matching what a validator would attest for).
        let pov_daa_score = query.pov_daa_score.unwrap_or_else(|| self.get_sink_daa_score_timestamp().daa_score);
        let records: Vec<StakeBondRecord> =
            self.storage.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        paginate_stake_bonds(records, &query, pov_daa_score)
    }

    fn get_active_validator_set(&self) -> Option<ActiveValidatorSet> {
        // kaspa-pq Phase 13 (ADR-0017): all active-bond validators attest every
        // epoch — there is no sortition committee. Return the full active set at
        // the sink (the pov is the sink DAA score so the epoch matches the
        // attestation target the validator will sign for).
        let dns_params = self.config.params.dns_params.as_ref()?;
        let pov_daa_score = self.get_sink_daa_score_timestamp().daa_score;
        let epoch = pov_daa_score / dns_params.epoch_length_blocks.max(1);

        let active = self.dns_active_validator_records(pov_daa_score);
        let active_validator_count = active.len();
        let mut members: Vec<_> = active.into_iter().map(|r| r.validator_id).collect();
        members.sort();
        Some(ActiveValidatorSet { epoch, pov_daa_score, active_validator_count, members })
    }

    fn get_mandatory_attestation_deficits(&self) -> Vec<MandatoryAttestationDeficit> {
        let Some(dns_params) = self.config.params.dns_params.as_ref() else {
            return Vec::new();
        };
        let virtual_daa_score = self.get_virtual_daa_score();
        if virtual_daa_score < dns_params.dns_activation_daa_score
            || virtual_daa_score < dns_params.mandatory_attestation_inclusion_daa_score
            || !dns_params.dns_v3_params_consistent()
        {
            return Vec::new();
        }

        let sink = self.get_sink();
        let anchors = self.virtual_processor.canonical_anchors_in_window(sink, dns_params);
        if anchors.is_empty() {
            return Vec::new();
        }

        let bonds: Vec<StakeBondRecord> =
            self.storage.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        let (contributions, _) = self.virtual_processor.collect_stake_contributions_v2(
            sink,
            None,
            &bonds,
            self.config.params.genesis.hash.as_byte_slice(),
            dns_params,
        );

        let mut seen = HashSet::new();
        let mut signed_by_epoch: HashMap<u64, u64> = HashMap::new();
        let mut contributed_by_epoch: HashMap<u64, Vec<MandatoryAttestationContributionKey>> = HashMap::new();
        for c in contributions {
            let key = (c.bond_outpoint, c.validator_id, c.epoch);
            if !seen.insert(key) {
                continue;
            }
            let entry = signed_by_epoch.entry(c.epoch).or_insert(0);
            *entry = entry.saturating_add(c.signed_stake_sompi);
            contributed_by_epoch.entry(c.epoch).or_default().push(MandatoryAttestationContributionKey {
                bond_outpoint: c.bond_outpoint,
                validator_id: c.validator_id,
                epoch: c.epoch,
            });
        }

        let mut deficits = Vec::new();
        for (&epoch, anchor) in &anchors {
            let mut active_validators: Vec<_> = bonds
                .iter()
                .filter(|bond| is_bond_active_at(bond, anchor.anchor_daa_score))
                .map(|bond| MandatoryAttestationValidator {
                    bond_outpoint: bond.bond_outpoint,
                    validator_id: bond.validator_pubkey_hash,
                    stake_sompi: bond.amount,
                })
                .collect();
            active_validators.sort_by(|a, b| {
                a.validator_id
                    .cmp(&b.validator_id)
                    .then(a.bond_outpoint.transaction_id.cmp(&b.bond_outpoint.transaction_id))
                    .then(a.bond_outpoint.index.cmp(&b.bond_outpoint.index))
            });

            let expected_stake = active_validators.iter().fold(0u64, |acc, v| acc.saturating_add(v.stake_sompi));
            if expected_stake == 0
                || expected_stake < dns_params.min_active_stake_sompi
                || (active_validators.len() as u32) < dns_params.min_active_validators
            {
                continue;
            }

            let parent_included_stake = signed_by_epoch.get(&epoch).copied().unwrap_or(0);
            if epoch_meets_quality_floor(
                parent_included_stake as u128,
                expected_stake as u128,
                dns_params.stake_event_quality_floor_bps,
            ) {
                continue;
            }

            let required_stake = required_stake_for_quality_floor(expected_stake, dns_params.stake_event_quality_floor_bps);
            deficits.push(MandatoryAttestationDeficit {
                epoch,
                target_hash: anchor.anchor_hash,
                target_daa_score: anchor.anchor_daa_score,
                validator_set_commitment: kaspa_consensus_core::Hash64::default(),
                pre_body_included_stake: parent_included_stake,
                expected_stake,
                required_stake,
                required_stake_delta: required_stake.saturating_sub(parent_included_stake),
                quality_floor_bps: dns_params.stake_event_quality_floor_bps,
                already_contributed: contributed_by_epoch.remove(&epoch).unwrap_or_default(),
                active_validators,
            });
        }

        deficits
    }

    fn get_attestation_quality_deficits(&self) -> Vec<AttestationQualityDeficit> {
        let Some(dns_params) = self.config.params.dns_params.as_ref() else {
            return Vec::new();
        };
        let virtual_daa_score = self.get_virtual_daa_score();
        if virtual_daa_score < dns_params.dns_activation_daa_score || !dns_params.dns_v3_params_consistent() {
            return Vec::new();
        }

        let sink = self.get_sink();
        let anchors = self.virtual_processor.canonical_anchors_in_window(sink, dns_params);
        if anchors.is_empty() {
            return Vec::new();
        }

        let bonds: Vec<StakeBondRecord> =
            self.storage.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        let (contributions, _) = self.virtual_processor.collect_stake_contributions_v2(
            sink,
            None,
            &bonds,
            self.config.params.genesis.hash.as_byte_slice(),
            dns_params,
        );

        let mut seen = HashSet::new();
        let mut signed_by_epoch: HashMap<u64, u64> = HashMap::new();
        for c in contributions {
            let key = (c.bond_outpoint, c.validator_id, c.epoch);
            if !seen.insert(key) {
                continue;
            }
            let entry = signed_by_epoch.entry(c.epoch).or_insert(0);
            *entry = entry.saturating_add(c.signed_stake_sompi);
        }

        let health = self.storage.dns_state_store.read().get().map(|state| state.health).unwrap_or_default();
        let mut deficits = Vec::new();
        for (&epoch, anchor) in &anchors {
            let active_validator_count = bonds.iter().filter(|bond| is_bond_active_at(bond, anchor.anchor_daa_score)).count();
            if (active_validator_count as u32) < dns_params.min_active_validators {
                continue;
            }
            let expected_stake = bonds
                .iter()
                .filter(|bond| is_bond_active_at(bond, anchor.anchor_daa_score))
                .fold(0u64, |acc, bond| acc.saturating_add(bond.amount));
            if expected_stake == 0 || expected_stake < dns_params.min_active_stake_sompi {
                continue;
            }

            let included_stake = signed_by_epoch.get(&epoch).copied().unwrap_or(0);
            if epoch_meets_quality_floor(included_stake as u128, expected_stake as u128, dns_params.stake_event_quality_floor_bps) {
                continue;
            }

            let required_stake = required_stake_for_quality_floor(expected_stake, dns_params.stake_event_quality_floor_bps);
            deficits.push(AttestationQualityDeficit {
                epoch,
                target_hash: anchor.anchor_hash,
                target_daa_score: anchor.anchor_daa_score,
                included_stake,
                expected_stake,
                required_stake,
                required_stake_delta: required_stake.saturating_sub(included_stake),
                quality_floor_bps: dns_params.stake_event_quality_floor_bps,
                health,
            });
        }

        deficits
    }

    fn get_validator_attestation_target(&self, bond_outpoint: TransactionOutpoint) -> Option<ValidatorAttestationTarget> {
        // kaspa-pq DNS v3: sign under-certified ready canonical anchors before already-certified
        // history so recovering validators improve future StakeScore quickly. If the ready window
        // is already above the quality floor, return the newest unsigned ready target so a
        // standalone validator does not stick to a long-certified epoch.
        self.get_validator_attestation_targets(bond_outpoint, 0, 1).into_iter().next()
    }

    fn get_validator_attestation_targets(
        &self,
        bond_outpoint: TransactionOutpoint,
        from_epoch: u64,
        limit: usize,
    ) -> Vec<ValidatorAttestationTarget> {
        // kaspa-pq DNS v3 (batch): scan only the current stake-score window, not `[0, lifetime]`.
        // Return under-certified epochs first in ascending order so optional hard-inclusion forks
        // can clear oldest-first and shipped liveness-first validators still improve stale
        // StakeScore. When there is no backlog, return the newest unsigned ready epochs so sidecars
        // keep up without repeatedly re-signing certified history.
        if limit == 0 {
            return Vec::new();
        }
        let Some(dns_params) = self.config.params.dns_params.as_ref() else {
            return Vec::new();
        };
        let Ok(bond) = self.storage.stake_bonds_store.read().get(&bond_outpoint).map(|r| (*r).clone()) else {
            return Vec::new();
        };
        let sink = self.get_sink();
        let Some(latest_ready) = ready_epoch_from_tip_blue_score(
            self.get_sink_blue_score(),
            dns_params.attestation_epoch_length_blue_score,
            dns_params.attestation_lag_blue_score,
        ) else {
            return Vec::new();
        };

        let bonds: Vec<StakeBondRecord> =
            self.storage.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
        let (contributions, _) = self.virtual_processor.collect_stake_contributions_v2(
            sink,
            None,
            &bonds,
            self.config.params.genesis.hash.as_byte_slice(),
            dns_params,
        );
        let mut seen = HashSet::new();
        let mut signed_by_epoch: HashMap<u64, u64> = HashMap::new();
        let mut signed_by_this_bond = HashSet::new();
        for c in contributions {
            if !seen.insert((c.bond_outpoint, c.validator_id, c.epoch)) {
                continue;
            }
            let entry = signed_by_epoch.entry(c.epoch).or_insert(0);
            *entry = entry.saturating_add(c.signed_stake_sompi);
            if c.bond_outpoint == bond_outpoint && c.validator_id == bond.validator_pubkey_hash {
                signed_by_this_bond.insert(c.epoch);
            }
        }

        let mut deficient = Vec::new();
        let mut fallback = Vec::new();
        for (epoch, anchor) in self.virtual_processor.canonical_anchors_in_window(sink, dns_params) {
            if epoch < from_epoch || epoch > latest_ready || signed_by_this_bond.contains(&epoch) {
                continue;
            }
            if !is_bond_active_at(&bond, anchor.anchor_daa_score) {
                continue;
            }
            let mut expected_stake = 0u64;
            let mut active_validator_count = 0u32;
            for b in bonds.iter().filter(|b| is_bond_active_at(b, anchor.anchor_daa_score)) {
                expected_stake = expected_stake.saturating_add(b.amount);
                active_validator_count = active_validator_count.saturating_add(1);
            }
            if expected_stake == 0
                || expected_stake < dns_params.min_active_stake_sompi
                || active_validator_count < dns_params.min_active_validators
            {
                continue;
            }
            let target = self.build_attestation_target(&anchor, bond_outpoint);
            let included = signed_by_epoch.get(&epoch).copied().unwrap_or(0);
            if epoch_meets_quality_floor(included as u128, expected_stake as u128, dns_params.stake_event_quality_floor_bps) {
                fallback.push(target);
            } else {
                deficient.push(target);
            }
        }

        if !deficient.is_empty() {
            deficient.into_iter().take(limit).collect()
        } else {
            fallback.into_iter().rev().take(limit).collect()
        }
    }

    fn get_sink_daa_score_timestamp(&self) -> DaaScoreTimestamp {
        let sink = self.get_sink();
        let compact = self.headers_store.get_compact_header_data(sink).unwrap();
        DaaScoreTimestamp { daa_score: compact.daa_score, timestamp: compact.timestamp }
    }

    fn get_current_block_color(&self, hash: BlockHash) -> Option<bool> {
        let _guard = self.pruning_lock.blocking_read();

        // Verify the block exists and can be assumed to have relations and reachability data
        self.validate_block_exists(hash).ok()?;

        // Verify that the block is in future(retention root), where Ghostdag data is complete
        self.services.reachability_service.is_dag_ancestor_of(self.get_retention_period_root(), hash).then_some(())?;

        let sink = self.get_sink();

        // Optimization: verify that the block is in past(sink), otherwise the search will fail anyway
        // (means the block was not merged yet by a virtual chain block)
        self.services.reachability_service.is_dag_ancestor_of(hash, sink).then_some(())?;

        let mut heap: BinaryHeap<Reverse<SortableBlock>> = BinaryHeap::new();
        let mut visited = BlockHashSet::new();

        let initial_children = self.get_block_children(hash).unwrap();

        for child in initial_children {
            if visited.insert(child) {
                let blue_work = self.ghostdag_store.get_blue_work(child).unwrap();
                heap.push(Reverse(SortableBlock::new(child, blue_work)));
            }
        }

        while let Some(Reverse(SortableBlock { hash: decedent, .. })) = heap.pop() {
            if self.services.reachability_service.is_chain_ancestor_of(decedent, sink) {
                let decedent_data = self.get_ghostdag_data(decedent).unwrap();

                if decedent_data.mergeset_blues.contains(&hash) {
                    return Some(true);
                } else if decedent_data.mergeset_reds.contains(&hash) {
                    return Some(false);
                }

                // Note: because we are doing a topological BFS up (from `hash` towards virtual), the first chain block
                // found must also be our merging block, so hash will be either in blues or in reds, rendering this line
                // unreachable.
                kaspa_core::warn!("DAG topology inconsistency: {decedent} is expected to be a merging block of {hash}");
                // TODO: we should consider the option of returning Result<Option<bool>> from this method
                return None;
            }

            let children = self.get_block_children(decedent).unwrap();

            for child in children {
                if visited.insert(child) {
                    let blue_work = self.ghostdag_store.get_blue_work(child).unwrap();
                    heap.push(Reverse(SortableBlock::new(child, blue_work)));
                }
            }
        }

        None
    }

    fn get_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        self.lkg_virtual_state.load().to_virtual_state_approx_id()
    }

    fn get_retention_period_root(&self) -> BlockHash {
        self.pruning_point_store.read().retention_period_root().unwrap()
    }

    /// Estimates the number of blocks and headers stored in the node database.
    ///
    /// This is an estimation based on the DAA score difference between the node's `retention root` and `virtual`'s DAA score,
    /// as such, it does not include non-daa blocks, and does not include headers stored as part of the pruning proof.
    fn estimate_block_count(&self) -> BlockCount {
        // PRUNE SAFETY: retention root is always a current or past pruning point which its header is kept permanently
        let retention_period_root_score = self.headers_store.get_daa_score(self.get_retention_period_root()).unwrap();
        let virtual_score = self.get_virtual_daa_score();
        // TODO(relaxed): change virtual's 0 daa initialization, and revert to normal subtraction
        let header_count = self
            .headers_store
            .get_daa_score(self.get_headers_selected_tip())
            .optional()
            .unwrap()
            .unwrap_or(virtual_score)
            .max(virtual_score)
            .saturating_sub(retention_period_root_score);
        let block_count = virtual_score.saturating_sub(retention_period_root_score);
        BlockCount { header_count, block_count }
    }

    fn get_virtual_chain_from_block(&self, low: BlockHash, chain_path_added_limit: Option<usize>) -> ConsensusResult<ChainPath> {
        // Calculate chain changes between the given `low` and the current sink hash (up to `limit` amount of block hashes).
        // Note:
        // 1) that we explicitly don't
        // do the calculation against the virtual itself so that we
        // won't later need to remove it from the result.
        // 2) supplying `None` as `chain_path_added_limit` will result in the full chain path, with optimized performance.
        let _guard = self.pruning_lock.blocking_read();

        // Verify that the block exists
        self.validate_block_exists(low)?;

        // Verify that retention root is on chain(block)
        self.services
            .reachability_service
            .is_chain_ancestor_of(self.get_retention_period_root(), low)
            .then_some(())
            .ok_or(ConsensusError::General("the queried hash does not have retention root on its chain"))?;

        Ok(self.services.dag_traversal_manager.calculate_chain_path(low, self.get_sink(), chain_path_added_limit))
    }

    /// Returns a Vec of header samples since genesis
    /// ordered by ascending daa_score, first entry is genesis
    fn get_chain_block_samples(&self) -> Vec<DaaScoreTimestamp> {
        // We need consistency between the past pruning points, selected chain and header store reads
        let _guard = self.pruning_lock.blocking_read();

        // Sorted from genesis to latest pruning_point_headers
        let pp_headers = self.pruning_point_compact_headers();
        let step_divisor: usize = 3; // The number of extra samples we'll get from blocks after last pp header
        let prealloc_len = pp_headers.len() + step_divisor + 1;

        let mut sample_headers;

        // Part 1: Add samples from pruning point headers:
        if self.config.net.network_type == NetworkType::Mainnet {
            // For mainnet, we add extra data (16 pp headers) from before checkpoint genesis.
            // Source: https://github.com/kaspagang/kaspad-py-explorer/blob/main/src/tx_timestamp_estimation.ipynb
            // For context see also: https://github.com/kaspagang/kaspad-py-explorer/blob/main/src/genesis_proof.ipynb
            const POINTS: &[DaaScoreTimestamp] = &[
                DaaScoreTimestamp { daa_score: 0, timestamp: 1636298787842 },
                DaaScoreTimestamp { daa_score: 87133, timestamp: 1636386662010 },
                DaaScoreTimestamp { daa_score: 176797, timestamp: 1636473700804 },
                DaaScoreTimestamp { daa_score: 264837, timestamp: 1636560706885 },
                DaaScoreTimestamp { daa_score: 355974, timestamp: 1636650005662 },
                DaaScoreTimestamp { daa_score: 445152, timestamp: 1636737841327 },
                DaaScoreTimestamp { daa_score: 536709, timestamp: 1636828600930 },
                DaaScoreTimestamp { daa_score: 624635, timestamp: 1636912614350 },
                DaaScoreTimestamp { daa_score: 712234, timestamp: 1636999362832 },
                DaaScoreTimestamp { daa_score: 801831, timestamp: 1637088292662 },
                DaaScoreTimestamp { daa_score: 890716, timestamp: 1637174890675 },
                DaaScoreTimestamp { daa_score: 978396, timestamp: 1637260956454 },
                DaaScoreTimestamp { daa_score: 1068387, timestamp: 1637349078269 },
                DaaScoreTimestamp { daa_score: 1139626, timestamp: 1637418723538 },
                DaaScoreTimestamp { daa_score: 1218320, timestamp: 1637495941516 },
                DaaScoreTimestamp { daa_score: 1312860, timestamp: 1637609671037 },
            ];
            sample_headers = Vec::<DaaScoreTimestamp>::with_capacity(prealloc_len + POINTS.len());
            sample_headers.extend_from_slice(POINTS);
        } else {
            sample_headers = Vec::<DaaScoreTimestamp>::with_capacity(prealloc_len);
        }

        for header in pp_headers.iter() {
            sample_headers.push(DaaScoreTimestamp { daa_score: header.1.daa_score, timestamp: header.1.timestamp });
        }

        // Part 2: Add samples from recent chain blocks
        let sc_read = self.storage.selected_chain_store.read();
        let high_index = sc_read.get_tip().unwrap().0;
        // The last pruning point is always expected in the selected chain store. However if due to some reason
        // this is not the case, we prefer not crashing but rather avoid sampling (hence set low index to high index)
        let low_index = sc_read.get_by_hash(pp_headers.last().unwrap().0).optional().unwrap().unwrap_or(high_index);
        let step_size = cmp::max((high_index - low_index) / (step_divisor as u64), 1);

        // We chain `high_index` to make sure we sample sink, and dedup to avoid sampling it twice
        for index in (low_index + step_size..=high_index).step_by(step_size as usize).chain(once(high_index)).dedup() {
            let compact = self
                .storage
                .headers_store
                .get_compact_header_data(sc_read.get_by_index(index).expect("store lock is acquired"))
                .unwrap();
            sample_headers.push(DaaScoreTimestamp { daa_score: compact.daa_score, timestamp: compact.timestamp });
        }

        sample_headers
    }
    fn get_transactions_by_accepting_daa_score(
        &self,
        accepting_daa_score: u64,
        tx_ids: Option<Vec<TransactionId>>,
        tx_type: TransactionType,
    ) -> ConsensusResult<TransactionQueryResult> {
        // We need consistency between the acceptance store and the block transaction store,
        let _guard = self.pruning_lock.blocking_read();
        let accepting_block = self
            .virtual_processor
            .find_accepting_chain_block_hash_at_daa_score(accepting_daa_score, self.get_retention_period_root())?;
        self.get_transactions_by_accepting_block(accepting_block, tx_ids, tx_type)
    }

    fn get_transactions_by_block_acceptance_data(
        &self,
        accepting_block: BlockHash,
        block_acceptance_data: MergesetBlockAcceptanceData,
        tx_ids: Option<Vec<TransactionId>>,
        tx_type: TransactionType,
    ) -> ConsensusResult<TransactionQueryResult> {
        // Need consistency between the acceptance store and the block transaction store.
        let _guard = self.pruning_lock.blocking_read();

        match tx_type {
            TransactionType::Transaction => {
                if let Some(tx_ids) = tx_ids {
                    let mut tx_ids_filter = HashSet::with_capacity(tx_ids.len());
                    tx_ids_filter.extend(tx_ids);

                    Ok(TransactionQueryResult::Transaction(Arc::new(
                        self.get_block_transactions(
                            block_acceptance_data.block_hash,
                            Some(
                                block_acceptance_data
                                    .accepted_transactions
                                    .into_iter()
                                    .filter_map(|atx| {
                                        if tx_ids_filter.contains(&atx.transaction_id) { Some(atx.index_within_block) } else { None }
                                    })
                                    .collect(),
                            ),
                        )?,
                    )))
                } else {
                    Ok(TransactionQueryResult::Transaction(Arc::new(self.get_block_transactions(
                        block_acceptance_data.block_hash,
                        Some(block_acceptance_data.accepted_transactions.iter().map(|atx| atx.index_within_block).collect()),
                    )?)))
                }
            }
            TransactionType::SignableTransaction => Ok(TransactionQueryResult::SignableTransaction(Arc::new(
                self.virtual_processor.get_populated_transactions_by_block_acceptance_data(
                    tx_ids,
                    block_acceptance_data,
                    accepting_block,
                )?,
            ))),
        }
    }

    fn get_transactions_by_accepting_block(
        &self,
        accepting_block: BlockHash,
        tx_ids: Option<Vec<TransactionId>>,
        tx_type: TransactionType,
    ) -> ConsensusResult<TransactionQueryResult> {
        // need consistency between the acceptance store and the block transaction store,
        let _guard = self.pruning_lock.blocking_read();

        match tx_type {
            TransactionType::Transaction => {
                let accepting_block_mergeset_acceptance_data_iter = self
                    .acceptance_data_store
                    .get(accepting_block)
                    .map_err(|_| ConsensusError::MissingData(accepting_block))?
                    .unwrap_or_clone()
                    .into_iter();

                if let Some(tx_ids) = tx_ids {
                    let mut tx_ids_filter = HashSet::with_capacity(tx_ids.len());
                    tx_ids_filter.extend(tx_ids);

                    Ok(TransactionQueryResult::Transaction(Arc::new(
                        accepting_block_mergeset_acceptance_data_iter
                            .flat_map(|mbad| {
                                self.get_block_transactions(
                                    mbad.block_hash,
                                    Some(
                                        mbad.accepted_transactions
                                            .into_iter()
                                            .filter_map(|atx| {
                                                if tx_ids_filter.contains(&atx.transaction_id) {
                                                    Some(atx.index_within_block)
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect(),
                                    ),
                                )
                            })
                            .flatten()
                            .collect::<Vec<_>>(),
                    )))
                } else {
                    Ok(TransactionQueryResult::Transaction(Arc::new(
                        accepting_block_mergeset_acceptance_data_iter
                            .flat_map(|mbad| {
                                self.get_block_transactions(
                                    mbad.block_hash,
                                    Some(mbad.accepted_transactions.iter().map(|atx| atx.index_within_block).collect()),
                                )
                            })
                            .flatten()
                            .collect::<Vec<_>>(),
                    )))
                }
            }
            TransactionType::SignableTransaction => Ok(TransactionQueryResult::SignableTransaction(Arc::new(
                self.virtual_processor.get_populated_transactions_by_accepting_block(tx_ids, accepting_block)?,
            ))),
        }
    }

    fn get_virtual_parents(&self) -> BlockHashSet {
        self.lkg_virtual_state.load().parents.iter().copied().collect()
    }

    fn get_virtual_parents_len(&self) -> usize {
        self.lkg_virtual_state.load().parents.len()
    }

    fn get_virtual_utxos(
        &self,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        let virtual_stores = self.virtual_stores.read();
        let iter = virtual_stores.utxo_set.seek_iterator(from_outpoint, chunk_size, skip_first);
        iter.map(|item| item.unwrap()).collect()
    }

    fn get_virtual_utxo_entry(&self, outpoint: TransactionOutpoint) -> Option<UtxoEntry> {
        // Seek to the first entry at-or-after `outpoint`; it is the requested
        // entry iff the key matches exactly (the outpoint is unspent).
        let virtual_stores = self.virtual_stores.read();
        virtual_stores
            .utxo_set
            .seek_iterator(Some(outpoint), 1, false)
            .next()
            .and_then(|item| item.ok())
            .filter(|(op, _)| *op == outpoint)
            .map(|(_, entry)| entry)
    }

    fn get_tips(&self) -> Vec<BlockHash> {
        self.body_tips_store.read().get().unwrap().read().iter().copied().collect_vec()
    }

    fn get_tips_len(&self) -> usize {
        self.body_tips_store.read().get().unwrap().read().len()
    }

    fn get_pruning_point_utxos(
        &self,
        expected_pruning_point: BlockHash,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> ConsensusResult<Vec<(TransactionOutpoint, UtxoEntry)>> {
        if self.pruning_point_store.read().pruning_point().unwrap() != expected_pruning_point {
            return Err(ConsensusError::UnexpectedPruningPoint);
        }
        let pruning_meta_read = self.pruning_meta_stores.read();
        let iter = pruning_meta_read.utxo_set.seek_iterator(from_outpoint, chunk_size, skip_first);
        let utxos = iter.map(|item| item.unwrap()).collect();
        drop(pruning_meta_read);

        // We recheck the expected pruning point in case it was switched just before the utxo set read.
        // NOTE: we rely on order of operations by pruning processor. See extended comment therein.
        if self.pruning_point_store.read().pruning_point().unwrap() != expected_pruning_point {
            return Err(ConsensusError::UnexpectedPruningPoint);
        }

        Ok(utxos)
    }

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        self.services.coinbase_manager.modify_coinbase_payload(payload, miner_data)
    }

    // PR-9.5c: trait signature widened to `MerkleRoot` (Hash64).
    fn calc_transaction_hash_merkle_root(&self, txs: &[Transaction]) -> kaspa_consensus_core::MerkleRoot {
        calc_hash_merkle_root(txs.iter())
    }

    fn validate_pruning_proof(
        &self,
        proof: &PruningPointProof,
        proof_metadata: &PruningProofMetadata,
    ) -> Result<(), PruningImportError> {
        self.services.pruning_proof_manager.validate_pruning_point_proof(proof, proof_metadata)
    }

    fn apply_pruning_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]) -> PruningImportResult<()> {
        self.services.pruning_proof_manager.apply_proof(proof, trusted_set)
    }

    fn import_pruning_points(&self, pruning_points: PruningPointsList) -> PruningImportResult<()> {
        self.services.pruning_proof_manager.import_pruning_points(&pruning_points)
    }

    fn append_imported_pruning_point_utxos(&self, utxoset_chunk: &[(TransactionOutpoint, UtxoEntry)], current_multiset: &mut MuHash) {
        let mut pruning_meta_write = self.pruning_meta_stores.write();
        pruning_meta_write.utxo_set.write_many(utxoset_chunk).unwrap();

        // Parallelize processing using the context of an existing thread pool.
        let inner_multiset = self.virtual_processor.install(|| {
            utxoset_chunk.par_iter().map(|(outpoint, entry)| MuHash::from_utxo(outpoint, entry)).reduce(MuHash::new, |mut a, b| {
                a.combine(&b);
                a
            })
        });

        current_multiset.combine(&inner_multiset);
    }

    fn import_pruning_point_utxo_set(&self, new_pruning_point: BlockHash, imported_utxo_multiset: MuHash) -> PruningImportResult<()> {
        self.virtual_processor.import_pruning_point_utxo_set(new_pruning_point, imported_utxo_multiset)
    }

    // kaspa-pq ADR-0022: pruned-IBD EVM + overlay snapshot transfer.
    fn check_evm_pruning_point_serveable(&self) -> kaspa_consensus_core::evm::EvmServeability {
        use kaspa_consensus_core::evm::EvmServeability;

        // Inert lane, or no pruning point yet: nothing to serve, not a fault.
        if self.config.evm_activation_daa_score == u64::MAX {
            return EvmServeability::NotApplicable;
        }
        let Ok(pruning_point) = self.pruning_point_store.read().pruning_point() else {
            return EvmServeability::NotApplicable;
        };

        // Whether an anchor / 206 already covers it — the cheap "Present" answer,
        // taken WITHOUT triggering a derivation.
        #[cfg(feature = "evm")]
        let present = {
            use crate::model::stores::evm::{EvmHeaderStoreReader, EvmStateCheckpointV2StoreReader, EvmStateStoreReader};
            // Pre-activation pruning point: nothing to serve.
            if self.storage.evm_header_store.get(pruning_point).is_err() {
                return EvmServeability::NotApplicable;
            }
            self.storage.evm_state_store.get(pruning_point).is_ok()
                || EvmStateCheckpointV2StoreReader::has(&*self.storage.evm_state_checkpoint_v2_store, pruning_point).unwrap_or(false)
        };
        #[cfg(not(feature = "evm"))]
        let present = self.virtual_processor.pruning_point_evm_state(pruning_point).is_some();

        if present {
            return EvmServeability::Present;
        }

        // Not directly present: ask the serving path, which DERIVES from the flat
        // head and caches on success (the proactive heal). `Some` ⇒ we just made
        // the node serveable ahead of any peer; `None` ⇒ it genuinely cannot serve.
        match self.virtual_processor.pruning_point_evm_state(pruning_point) {
            Some(_) => EvmServeability::Derived,
            None => EvmServeability::Unserveable,
        }
    }

    fn pruning_point_evm_state(
        &self,
        pruning_point: BlockHash,
    ) -> Option<(kaspa_consensus_core::evm::EvmExecutionHeader, kaspa_consensus_core::evm::EvmStateSnapshot)> {
        self.virtual_processor.pruning_point_evm_state(pruning_point)
    }

    fn import_pruning_point_evm_state(
        &self,
        pruning_point: BlockHash,
        evm_header: kaspa_consensus_core::evm::EvmExecutionHeader,
        snapshot: kaspa_consensus_core::evm::EvmStateSnapshot,
    ) -> PruningImportResult<()> {
        self.virtual_processor.import_pruning_point_evm_state(pruning_point, evm_header, snapshot)
    }

    fn pruning_point_overlay_snapshot(&self) -> Option<kaspa_consensus_core::dns_finality::PruningPointOverlaySnapshot> {
        self.virtual_processor.pruning_point_overlay_snapshot()
    }

    fn import_pruning_point_overlay_snapshot(
        &self,
        pruning_point: BlockHash,
        snapshot: kaspa_consensus_core::dns_finality::OverlaySnapshot,
    ) -> PruningImportResult<()> {
        self.virtual_processor.import_pruning_point_overlay_snapshot(pruning_point, snapshot)
    }

    fn validate_pruning_points(&self, syncer_virtual_selected_parent: BlockHash) -> ConsensusResult<()> {
        let hst = self.storage.headers_selected_tip_store.read().get().unwrap().hash;
        let (synced_pruning_point, synced_pp_index) = self.pruning_point_store.read().pruning_point_and_index().unwrap();
        if !self.services.pruning_point_manager.is_valid_pruning_point(synced_pruning_point, hst) {
            return Err(ConsensusError::General("pruning point does not coincide with the synced header selected tip"));
        }
        if !self.services.pruning_point_manager.is_valid_pruning_point(synced_pruning_point, syncer_virtual_selected_parent) {
            return Err(ConsensusError::General("pruning point does not coincide with the syncer's sink (virtual selected parent)"));
        }
        self.services
            .pruning_point_manager
            .are_pruning_points_in_valid_chain(synced_pruning_point, synced_pp_index, syncer_virtual_selected_parent)
            .map_err(|e| ConsensusError::GeneralOwned(format!("past pruning points do not form a valid chain: {}", e)))
    }

    fn is_chain_ancestor_of(&self, low: BlockHash, high: BlockHash) -> ConsensusResult<bool> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(low)?;
        self.validate_block_exists(high)?;
        Ok(self.services.reachability_service.is_chain_ancestor_of(low, high))
    }

    // max_blocks has to be greater than the merge set size limit
    fn get_hashes_between(&self, low: BlockHash, high: BlockHash, max_blocks: usize) -> ConsensusResult<(Vec<BlockHash>, BlockHash)> {
        let _guard = self.pruning_lock.blocking_read();
        assert!(max_blocks as u64 > self.config.mergeset_size_limit());
        self.validate_block_exists(low)?;
        self.validate_block_exists(high)?;

        Ok(self.services.sync_manager.antipast_hashes_between(low, high, Some(max_blocks)))
    }

    fn get_header(&self, hash: BlockHash) -> ConsensusResult<Arc<Header>> {
        self.headers_store.get_header(hash).optional().unwrap().ok_or(ConsensusError::HeaderNotFound(hash))
    }

    fn get_headers_selected_tip(&self) -> BlockHash {
        self.headers_selected_tip_store.read().get().unwrap().hash
    }

    fn get_antipast_from_pov(
        &self,
        hash: BlockHash,
        context: BlockHash,
        max_traversal_allowed: Option<u64>,
    ) -> ConsensusResult<Vec<BlockHash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(hash)?;
        self.validate_block_exists(context)?;
        Ok(self.services.dag_traversal_manager.antipast(hash, std::iter::once(context), max_traversal_allowed)?)
    }

    fn get_anticone(&self, hash: BlockHash) -> ConsensusResult<Vec<BlockHash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(hash)?;
        let virtual_state = self.lkg_virtual_state.load();
        Ok(self.services.dag_traversal_manager.anticone(hash, virtual_state.parents.iter().copied(), None)?)
    }

    fn get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        // PRUNE SAFETY: proof is cached before the prune op begins and the
        // pruning point cannot move during the prune so the cache remains valid
        self.services.pruning_proof_manager.get_pruning_point_proof()
    }

    fn create_virtual_selected_chain_block_locator(
        &self,
        low: Option<BlockHash>,
        high: Option<BlockHash>,
    ) -> ConsensusResult<Vec<BlockHash>> {
        let _guard = self.pruning_lock.blocking_read();
        if let Some(low) = low {
            self.validate_block_exists(low)?;
        }

        if let Some(high) = high {
            self.validate_block_exists(high)?;
        }

        Ok(self.services.sync_manager.create_virtual_selected_chain_block_locator(low, high)?)
    }

    fn pruning_point_headers(&self) -> Vec<Arc<Header>> {
        // PRUNE SAFETY: index is monotonic and past pruning point headers are expected permanently
        let (pruning_point, pruning_index) = self.pruning_point_store.read().pruning_point_and_index().unwrap();
        (0..pruning_index)
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .chain(once(pruning_point))
            .map(|hash| self.headers_store.get_header(hash).unwrap())
            .collect_vec()
    }

    fn get_pruning_point_anticone_and_trusted_data(&self) -> ConsensusResult<Arc<PruningPointTrustedData>> {
        // PRUNE SAFETY: anticone and trusted data are cached before the prune op begins and the
        // pruning point cannot move during the prune so the cache remains valid
        self.services.pruning_proof_manager.get_pruning_point_anticone_and_trusted_data()
    }

    fn get_block(&self, hash: BlockHash) -> ConsensusResult<Block> {
        if match self.statuses_store.read().get(hash).optional().unwrap() {
            Some(status) => !status.has_block_body(),
            None => true,
        } {
            return Err(ConsensusError::BlockNotFound(hash));
        }

        Ok(Block {
            header: self.headers_store.get_header(hash).optional().unwrap().ok_or(ConsensusError::BlockNotFound(hash))?,
            transactions: self.block_transactions_store.get(hash).optional().unwrap().ok_or(ConsensusError::BlockNotFound(hash))?,
            // kaspa-pq EVM Lane v0.4 (§3.1): the block's own payload (absent
            // store row = the empty payload) — getBlock RPC and the IBD
            // full-block server must serve the bytes `evm_payload_hash`
            // commits to, or a served v2 block fails the receiver's body rule.
            evm_payload: Arc::new(self.get_block_evm_payload(hash)?),
        })
    }

    fn get_block_transactions(
        &self,
        hash: BlockHash,
        indices: Option<Vec<TransactionIndexType>>,
    ) -> ConsensusResult<Vec<Transaction>> {
        let transactions = self.block_transactions_store.get(hash).optional().unwrap().ok_or(ConsensusError::BlockNotFound(hash))?;
        let tx_len = transactions.len();

        if let Some(indices) = indices {
            if tx_len < indices.len() {
                return Err(ConsensusError::TransactionQueryTooLarge(indices.len(), hash, transactions.len()));
            }

            let res = transactions
                .unwrap_or_clone()
                .into_iter()
                .enumerate()
                .filter(|(index, _tx)| indices.contains(&(*index as TransactionIndexType)))
                .map(|(_, tx)| tx)
                .collect::<Vec<_>>();

            if res.len() != indices.len() {
                Err(ConsensusError::TransactionIndexOutOfBounds(*indices.iter().max().unwrap(), tx_len, hash))
            } else {
                Ok(res)
            }
        } else {
            Ok(transactions.unwrap_or_clone())
        }
    }

    fn get_block_body(&self, hash: BlockHash) -> ConsensusResult<Arc<Vec<Transaction>>> {
        if match self.statuses_store.read().get(hash).optional().unwrap() {
            Some(status) => !status.has_block_body(),
            None => true,
        } {
            return Err(ConsensusError::BlockNotFound(hash));
        }

        self.block_transactions_store.get(hash).optional().unwrap().ok_or(ConsensusError::BlockNotFound(hash))
    }

    fn get_evm_tx_locations(&self, tx_hash: kaspa_hashes::EvmH256) -> ConsensusResult<kaspa_consensus_core::evm::EvmTxLocations> {
        Ok(self.storage.evm_tx_index_store.get_or_default(tx_hash).unwrap())
    }

    fn get_evm_tx_receipt(
        &self,
        tx_hash: kaspa_hashes::EvmH256,
    ) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmTxReceiptView>> {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmReceiptsStoreReader};
        let row = self.storage.evm_tx_index_store.get_or_default(tx_hash).unwrap();
        for (accepting, receipt_index) in row.accepted_in.iter().rev() {
            // Canonical resolution: only an acceptance on the CURRENT selected
            // chain counts (§16 — orphaned receipts read as null at `latest`).
            if !self.is_chain_block(*accepting).unwrap_or(false) {
                continue;
            }
            let receipts = self.storage.evm_receipts_store.get(*accepting).optional().unwrap().unwrap_or_default();
            let idx = *receipt_index as usize;
            if idx >= receipts.receipts.len() || receipts.tx_hashes.get(idx) != Some(&tx_hash) {
                continue; // defensive: index row out of sync with the receipts row
            }
            let evm_number =
                self.storage.evm_header_store.get(*accepting).optional().unwrap().map(|h| h.evm_number).unwrap_or_default();
            // Block-global offset of this receipt's first log (audit H-05): the
            // count of all logs in the receipts before this one in the block.
            let log_index_offset: u32 = receipts.receipts[..idx].iter().map(|r| r.logs.len() as u32).sum();
            return Ok(Some(kaspa_consensus_core::evm::EvmTxReceiptView {
                accepting_block: *accepting,
                evm_number,
                receipt_index: *receipt_index,
                log_index_offset,
                receipt: receipts.receipts[idx].clone(),
            }));
        }
        Ok(None)
    }

    fn get_evm_head_header(&self) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmExecutionHeader>> {
        use crate::model::stores::evm::EvmHeaderStoreReader;
        Ok(self.storage.evm_header_store.get(self.get_sink()).optional().unwrap())
    }

    fn get_evm_header_of(&self, block: BlockHash) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmExecutionHeader>> {
        use crate::model::stores::evm::EvmHeaderStoreReader;
        Ok(self.storage.evm_header_store.get(block).optional().unwrap())
    }

    fn get_evm_canonical_heads(&self) -> ConsensusResult<Option<kaspa_consensus_core::evm::CanonicalEvmHeads>> {
        use crate::model::stores::evm::EvmCanonicalHeadsStoreReader;
        // Absent (pre-activation / non-EVM) reads as None rather than an error.
        Ok(self.storage.evm_heads_store.read().get().optional().unwrap())
    }

    fn get_evm_state_snapshot_of(&self, block: BlockHash) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmStateSnapshot>> {
        use crate::model::stores::evm::EvmStateStoreReader;
        // Hot path: the per-block 206 snapshot (present on every node that persists it — the default).
        if let Some(snapshot) = self.storage.evm_state_store.get(block).optional().unwrap() {
            return Ok(Some(snapshot));
        }
        // C-01 S9b: 206 was retired (--evm-retire-206) or this block was committed while retired. Serve
        // the state from the flat backend instead — materialize it directly when `block` is the flat
        // canonical head (exact, O(state)), else §12-reconstruct (root-verified). This keeps eth_call /
        // trace / account reads working without the 206 store. Read-path only; behavior-preserving when
        // 206 is present (returned above) and on inert/non-EVM nets (no flat head ⇒ reconstruct ⇒ None).
        #[cfg(feature = "evm")]
        if let Ok(Some(ptr)) = self.storage.evm_latest_state_ptr_store.read().get()
            && ptr.canonical_head == block
        {
            let snap = crate::processes::evm::materialize_snapshot(&self.storage.evm_flat_account_store, &self.storage.evm_code_store)
                .map_err(|e| kaspa_consensus_core::errors::consensus::ConsensusError::GeneralOwned(e.to_string()))?;
            return Ok(Some(snap));
        }
        self.reconstruct_evm_state_at(block)
    }

    fn get_evm_trace_replay_body(&self, block: BlockHash) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmTraceReplayBodyV1>> {
        use crate::model::stores::evm::EvmTraceReplayStoreReader;
        // The store's `get` already maps an absent key to `Ok(None)`. A real store
        // fault (RocksDB I/O / borsh corruption) surfaces as a clean consensus error
        // the RPC layer turns into a JSON-RPC error — never a serving-task panic.
        self.storage
            .evm_trace_store
            .get(block)
            .map_err(|e| kaspa_consensus_core::errors::consensus::ConsensusError::GeneralOwned(e.to_string()))
    }

    fn evm_activation_fences(&self) -> (u64, u64, u64) {
        (
            self.config.params.evm_gas_pool_v2_activation_daa_score,
            self.config.params.evm_f002_withdraw_cap_activation_daa_score,
            self.config.params.evm_f003_mldsa_verify_activation_daa_score,
        )
    }

    fn reconstruct_evm_state_at(&self, block: BlockHash) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmStateSnapshot>> {
        use crate::model::stores::evm::EvmHeaderStoreReader;
        use kaspa_consensus_core::errors::consensus::ConsensusError;

        // Not an EVM block (no committed header) ⇒ None — distinct from "EVM block
        // whose state history this node doesn't retain", which is an Err below.
        let Some(target_header) = self.storage.evm_header_store.get(block).optional().unwrap() else {
            return Ok(None);
        };

        #[cfg(feature = "evm")]
        {
            use crate::model::stores::evm::EvmStateDiffStoreReader;
            let oops = |m: String| ConsensusError::GeneralOwned(m);

            // Walk `block`'s selected-parent chain backward (design §12.4) to the
            // nearest checkpoint (its full state) or the pre-activation genesis,
            // collecting the forward diffs to replay. Pure store-walk.
            let (seed, forward_diffs) = crate::processes::evm::gather_reconstruction_inputs(
                block,
                |b| {
                    crate::processes::evm::resolve_anchor_snapshot(
                        b,
                        &self.storage.evm_state_checkpoint_v2_store,
                        &self.storage.evm_state_checkpoint_store,
                        &self.storage.evm_code_store,
                    )
                },
                |b| self.storage.evm_state_diff_store.get(b),
                |b| {
                    crate::processes::evm::classify_parent_for_anchor(
                        b,
                        self.config.evm_activation_daa_score,
                        |h| self.storage.evm_header_store.get(h).optional().unwrap().is_some(),
                        |h| self.storage.headers_store.get_daa_score(h).ok(),
                    )
                },
            )
            .map_err(|e| oops(e.to_string()))?;

            // Reconstruct + verify the keccak-MPT root against the committed state root.
            let snapshot = kaspa_evm::reconstruct::reconstruct_evm_state(
                &seed,
                &forward_diffs,
                |h| {
                    use crate::model::stores::evm::EvmCodeStoreReader;
                    self.storage.evm_code_store.get(*h).ok().flatten()
                },
                target_header.state_root,
            )
            .map_err(|e| oops(format!("EVM reconstruction of {block}: {e}")))?;
            Ok(Some(snapshot))
        }
        #[cfg(not(feature = "evm"))]
        {
            let _ = target_header;
            Err(ConsensusError::GeneralOwned("EVM historical state reconstruction requires an evm-feature node (revm)".into()))
        }
    }

    fn get_evm_flat_account_at_head(
        &self,
        address: kaspa_consensus_core::evm::EvmAddress,
    ) -> ConsensusResult<kaspa_consensus_core::evm::FlatHeadAccount> {
        use crate::model::stores::evm::EvmCodeStoreReader;
        use kaspa_consensus_core::evm::{EVM_EMPTY_CODE_HASH, FlatHeadAccount};
        // Trust the flat rows ONLY when the latest pointer (231) is the current sink:
        // the shadow dual-write advances the flat rows + pointer atomically per commit
        // (S4) and re-bases both together on reorg (S5), so `ptr.canonical_head == sink`
        // ⇔ the flat rows materialize the head. An absent pointer (shadow backend never
        // wrote it), a stale pointer (shadow disabled, or a re-base mid-flight), or any
        // flat-store read hiccup ⇒ `Stale` ⇒ the caller falls back to the authoritative
        // full-snapshot path. The flat fast path is never authoritative on its own.
        let Ok(Some(ptr)) = self.storage.evm_latest_state_ptr_store.read().get() else {
            return Ok(FlatHeadAccount::Stale);
        };
        if ptr.canonical_head != self.get_sink() {
            return Ok(FlatHeadAccount::Stale);
        }
        let flat = match self.storage.evm_flat_account_store.get(address) {
            Ok(Some(flat)) => flat,
            // Flat store is at the head and has no row for this address ⇒ the account
            // does not exist at head (authoritative for this query).
            Ok(None) => return Ok(FlatHeadAccount::AtHead(None)),
            Err(_) => return Ok(FlatHeadAccount::Stale),
        };
        // Resolve code via the content-addressed code store (222); an EOA's
        // `KECCAK_EMPTY` needs no lookup. A referenced-but-missing code row ⇒ fall back
        // (the authoritative snapshot inlines code) rather than report empty code.
        let code = if flat.core.code_hash == EVM_EMPTY_CODE_HASH {
            Vec::new()
        } else {
            match self.storage.evm_code_store.get(flat.core.code_hash) {
                Ok(Some(code)) => code,
                Ok(None) | Err(_) => return Ok(FlatHeadAccount::Stale),
            }
        };
        Ok(FlatHeadAccount::AtHead(Some(flat.to_snapshot(address, code))))
    }

    fn get_evm_block_by_l1_hash(&self, l1_hash: BlockHash) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmBlockResponse>> {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmRawTxStoreReader, EvmReceiptsStoreReader};
        let Some(header) = self.storage.evm_header_store.get(l1_hash).optional().unwrap() else { return Ok(None) };
        let tx_hashes = self.storage.evm_receipts_store.get(l1_hash).optional().unwrap().map(|r| r.tx_hashes).unwrap_or_default();
        // RPC §7.3 `size`: byte length of the block's accepted tx data (sum of raw
        // EIP-2718 bytes via the R-2 raw-tx store; an absent row contributes 0).
        let encoded_size =
            tx_hashes.iter().map(|h| self.storage.evm_raw_tx_store.get(*h).unwrap().map(|r| r.raw.len() as u64).unwrap_or(0)).sum();
        Ok(Some(kaspa_consensus_core::evm::EvmBlockResponse { header, l1_hash, tx_hashes, encoded_size }))
    }

    fn get_evm_block_logs(&self, l1_hash: BlockHash) -> ConsensusResult<Vec<kaspa_consensus_core::evm::EvmLogEntry>> {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmReceiptsStoreReader};
        // Read by L1 hash from the IMMUTABLE header + receipts stores (never the
        // reorg-mutable number map): the §9 logs reorg pump emits detached blocks,
        // which are no longer canonical but whose receipts are still stored. No
        // canonical filter here — the pump tags removed=true/false itself.
        let Some(header) = self.storage.evm_header_store.get(l1_hash).optional().unwrap() else { return Ok(Vec::new()) };
        let receipts = self.storage.evm_receipts_store.get(l1_hash).optional().unwrap().unwrap_or_default();
        let mut out = Vec::new();
        let mut log_index: u32 = 0;
        for (rcpt_idx, receipt) in receipts.receipts.iter().enumerate() {
            let tx_hash = receipts.tx_hashes.get(rcpt_idx).copied().unwrap_or_default();
            for log in &receipt.logs {
                out.push(kaspa_consensus_core::evm::EvmLogEntry {
                    address: log.address,
                    topics: log.topics.clone(),
                    data: log.data.clone(),
                    block_number: header.evm_number,
                    block_l1_hash: l1_hash,
                    tx_hash,
                    tx_index: rcpt_idx as u32,
                    log_index,
                });
                log_index += 1;
            }
        }
        Ok(out)
    }

    fn get_evm_raw_tx(&self, tx_hash: kaspa_hashes::EvmH256) -> ConsensusResult<Option<Vec<u8>>> {
        use crate::model::stores::evm::EvmRawTxStoreReader;
        Ok(self.storage.evm_raw_tx_store.get(tx_hash).unwrap().map(|r| r.raw))
    }

    fn get_evm_block_by_number(&self, evm_number: u64) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmBlockResponse>> {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmNumberStoreReader};
        // Resolve the (upsert) number index, then re-validate canonicality: the
        // candidate must still be a selected-chain block AND its header's
        // evm_number must match (a reorg-orphaned row reads as absent — the same
        // canonical-resolution guard as `get_evm_tx_receipt`).
        let Some(l1_hash) = self.storage.evm_number_store.get(evm_number).unwrap() else { return Ok(None) };
        if !self.is_chain_block(l1_hash).unwrap_or(false) {
            return Ok(None);
        }
        match self.storage.evm_header_store.get(l1_hash).optional().unwrap() {
            Some(h) if h.evm_number == evm_number => self.get_evm_block_by_l1_hash(l1_hash),
            _ => Ok(None),
        }
    }

    fn get_evm_block_by_rpc_hash(
        &self,
        rpc_hash: kaspa_hashes::EvmH256,
    ) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmBlockResponse>> {
        use crate::model::stores::evm::EvmBlockHashMapStoreReader;
        let Some(l1_hash) = self.storage.evm_block_hash_map_store.get(rpc_hash).unwrap() else { return Ok(None) };
        self.get_evm_block_by_l1_hash(l1_hash)
    }

    fn get_evm_logs(
        &self,
        from_number: u64,
        to_number: u64,
        addresses: Vec<kaspa_consensus_core::evm::EvmAddress>,
        topics: Vec<Vec<kaspa_hashes::EvmH256>>,
    ) -> ConsensusResult<Vec<kaspa_consensus_core::evm::EvmLogEntry>> {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmNumberStoreReader, EvmReceiptsStoreReader};
        // DoS bound: cap the result set (the crate caps the block range upstream).
        // Exceeding the cap is an ERROR, not a silent truncation (audit H-05): a
        // truncated array indistinguishable from a complete one makes indexers
        // drop Transfer/Mint logs and misreport ownership/supply. Callers must
        // narrow the range or filters (EIP-1474 "query returned more than N").
        const MAX_LOGS: usize = 10_000;
        if to_number < from_number {
            return Ok(Vec::new());
        }
        // `topics[i]` non-empty ⇒ the log's i-th topic must be one of them; empty ⇒ wildcard.
        let topic_match = |log_topics: &[kaspa_hashes::EvmH256]| -> bool {
            for (i, allowed) in topics.iter().enumerate() {
                if allowed.is_empty() {
                    continue;
                }
                match log_topics.get(i) {
                    Some(t) if allowed.contains(t) => {}
                    _ => return false,
                }
            }
            true
        };

        // §8 fast path: when the query filters by address AND the posting index is
        // known complete for the range (`from_number >= indexed_floor`), seed from
        // the address posting index instead of scanning every block. The floor gate
        // prevents silently missing logs from blocks indexed before the writer was
        // deployed (a backfill lowers the floor — design §14).
        if !addresses.is_empty() && self.storage.evm_log_index_store.indexed_floor().is_some_and(|f| from_number >= f) {
            let mut out: Vec<kaspa_consensus_core::evm::EvmLogEntry> = Vec::new();
            let mut seen: std::collections::HashSet<[u8; 20]> = std::collections::HashSet::new();
            for addr in addresses.iter().copied() {
                if !seen.insert(addr.as_bytes()) {
                    continue; // a log has one address — dedup duplicate seeds
                }
                // Collect this address's in-range postings (ascending block order),
                // then resolve each (the iterator borrows the store).
                let locs: Vec<_> = self
                    .storage
                    .evm_log_index_store
                    .bucket_locs(kaspa_consensus_core::evm::LogPostingKind::Address, &addr.as_bytes())
                    .skip_while(|loc| loc.evm_number < from_number)
                    .take_while(|loc| loc.evm_number <= to_number)
                    .collect();
                for loc in locs {
                    // Canonical-resolve the posting (drop side-branch entries) — the
                    // same backstop get_evm_block_by_number uses.
                    if !self.is_chain_block(loc.l1_hash).unwrap_or(false) {
                        continue;
                    }
                    let Some(header) = self.storage.evm_header_store.get(loc.l1_hash).optional().unwrap() else { continue };
                    if header.evm_number != loc.evm_number {
                        continue;
                    }
                    let receipts = self.storage.evm_receipts_store.get(loc.l1_hash).optional().unwrap().unwrap_or_default();
                    let Some(receipt) = receipts.receipts.get(loc.tx_index as usize) else { continue };
                    let Some(log) = receipt.logs.get(loc.in_receipt_log_index as usize) else { continue };
                    if !topic_match(&log.topics) {
                        continue;
                    }
                    // Block-global logIndex = logs in earlier receipts + in-receipt index.
                    let prior: u32 = receipts.receipts[..loc.tx_index as usize].iter().map(|r| r.logs.len() as u32).sum();
                    let tx_hash = receipts.tx_hashes.get(loc.tx_index as usize).copied().unwrap_or_default();
                    out.push(kaspa_consensus_core::evm::EvmLogEntry {
                        address: log.address,
                        topics: log.topics.clone(),
                        data: log.data.clone(),
                        block_number: loc.evm_number,
                        block_l1_hash: loc.l1_hash,
                        tx_hash,
                        tx_index: loc.tx_index,
                        log_index: prior + loc.in_receipt_log_index,
                    });
                    if out.len() > MAX_LOGS {
                        return Err(ConsensusError::GeneralOwned(format!(
                            "eth_getLogs: query matched more than {MAX_LOGS} logs in block range [{from_number},{to_number}]; narrow the range or filters"
                        )));
                    }
                }
            }
            // Address buckets interleave by block → sort to canonical order.
            out.sort_by_key(|e| (e.block_number, e.tx_index, e.log_index));
            return Ok(out);
        }

        let mut out = Vec::new();
        for n in from_number..=to_number {
            let Some(l1_hash) = self.storage.evm_number_store.get(n).unwrap() else { continue };
            // Reorg-validate the (upsert) number index before trusting the row.
            if !self.is_chain_block(l1_hash).unwrap_or(false) {
                continue;
            }
            let Some(header) = self.storage.evm_header_store.get(l1_hash).optional().unwrap() else { continue };
            if header.evm_number != n {
                continue;
            }
            let receipts = self.storage.evm_receipts_store.get(l1_hash).optional().unwrap().unwrap_or_default();
            let mut log_index: u32 = 0;
            for (rcpt_idx, receipt) in receipts.receipts.iter().enumerate() {
                let tx_hash = receipts.tx_hashes.get(rcpt_idx).cloned().unwrap_or_default();
                for log in &receipt.logs {
                    let li = log_index;
                    log_index += 1;
                    if !addresses.is_empty() && !addresses.contains(&log.address) {
                        continue;
                    }
                    if !topic_match(&log.topics) {
                        continue;
                    }
                    out.push(kaspa_consensus_core::evm::EvmLogEntry {
                        address: log.address,
                        topics: log.topics.clone(),
                        data: log.data.clone(),
                        block_number: n,
                        block_l1_hash: l1_hash,
                        tx_hash,
                        tx_index: rcpt_idx as u32,
                        log_index: li,
                    });
                    if out.len() > MAX_LOGS {
                        return Err(ConsensusError::GeneralOwned(format!(
                            "eth_getLogs: query matched more than {MAX_LOGS} logs in block range [{from_number},{to_number}]; narrow the range or filters"
                        )));
                    }
                }
            }
        }
        Ok(out)
    }

    fn get_block_evm_payload(&self, hash: BlockHash) -> ConsensusResult<kaspa_consensus_core::evm::EvmExecutionPayload> {
        // kaspa-pq EVM Lane v0.4 (§3.1): the payload store only holds rows for
        // non-empty payloads (commit_body persists them), so absence is the
        // empty payload — every pre-activation block and every v2 block whose
        // producer carried no EVM data.
        use crate::model::stores::evm::EvmPayloadStoreReader;
        Ok(self.storage.evm_payload_store.get(hash).optional().unwrap().unwrap_or_default())
    }

    fn get_block_even_if_header_only(&self, hash: BlockHash) -> ConsensusResult<Block> {
        let Some(status) = self.statuses_store.read().get(hash).optional().unwrap().filter(|&status| status.has_block_header()) else {
            return Err(ConsensusError::HeaderNotFound(hash));
        };
        Ok(Block {
            header: self.headers_store.get_header(hash).optional().unwrap().ok_or(ConsensusError::HeaderNotFound(hash))?,
            transactions: if status.is_header_only() {
                Default::default()
            } else {
                self.block_transactions_store.get(hash).optional().unwrap().unwrap_or_default()
            },
            // kaspa-pq EVM Lane v0.4 (§3.1): a header-only block has no body and
            // therefore no payload row — `get_block_evm_payload` maps the absent
            // row to the empty payload, mirroring the tolerant transactions read.
            evm_payload: Arc::new(self.get_block_evm_payload(hash)?),
        })
    }

    fn get_ghostdag_data(&self, hash: BlockHash) -> ConsensusResult<ExternalGhostdagData> {
        match self.get_block_status(hash) {
            None => return Err(ConsensusError::HeaderNotFound(hash)),
            Some(BlockStatus::StatusInvalid) => return Err(ConsensusError::InvalidBlock(hash)),
            _ => {}
        };
        let ghostdag = self.ghostdag_store.get_data(hash).optional().unwrap().ok_or(ConsensusError::MissingData(hash))?;
        Ok((&*ghostdag).into())
    }

    fn get_block_children(&self, hash: BlockHash) -> Option<Vec<BlockHash>> {
        self.services
            .relations_service
            .get_children(hash)
            .optional()
            .unwrap()
            .map(|children| children.read().iter().copied().collect_vec())
    }

    fn get_block_parents(&self, hash: BlockHash) -> Option<Arc<Vec<BlockHash>>> {
        self.services.relations_service.get_parents(hash).optional().unwrap()
    }

    fn get_block_status(&self, hash: BlockHash) -> Option<BlockStatus> {
        self.statuses_store.read().get(hash).optional().unwrap()
    }

    fn get_block_acceptance_data(&self, hash: BlockHash) -> ConsensusResult<Arc<AcceptanceData>> {
        self.acceptance_data_store.get(hash).optional().unwrap().ok_or(ConsensusError::MissingData(hash))
    }

    fn get_blocks_acceptance_data(
        &self,
        hashes: &[BlockHash],
        merged_blocks_limit: Option<usize>,
    ) -> ConsensusResult<Vec<Arc<AcceptanceData>>> {
        // Note: merged_blocks_limit will limit after the sum of merged blocks is breached along the supplied hash's acceptance data
        // and not limit the acceptance data within a queried hash. i.e. It has mergeset_size_limit granularity, this is to guarantee full acceptance data coverage.
        if merged_blocks_limit.is_none() {
            return hashes
                .iter()
                .copied()
                .map(|hash| self.acceptance_data_store.get(hash).optional().unwrap().ok_or(ConsensusError::MissingData(hash)))
                .collect::<ConsensusResult<Vec<_>>>();
        }
        let merged_blocks_limit = merged_blocks_limit.unwrap(); // we handle `is_none`, so may unwrap.
        let mut num_of_merged_blocks = 0usize;

        hashes
            .iter()
            .copied()
            .map_while(|hash| {
                let entry = self.acceptance_data_store.get(hash).optional().unwrap().ok_or(ConsensusError::MissingData(hash));
                num_of_merged_blocks += entry.as_ref().map_or(0, |entry| entry.len());
                if num_of_merged_blocks > merged_blocks_limit { None } else { Some(entry) }
            })
            .collect::<ConsensusResult<Vec<_>>>()
    }

    fn is_chain_block(&self, hash: BlockHash) -> ConsensusResult<bool> {
        self.is_chain_ancestor_of(hash, self.get_sink())
    }

    fn get_missing_block_body_hashes(&self, high: BlockHash) -> ConsensusResult<Vec<BlockHash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(high)?;
        Ok(self.services.sync_manager.get_missing_block_body_hashes(high)?)
    }
    /// Returns the set of blocks in the anticone of the current pruning point
    /// which (may) lack a block body due to being in a transitional state
    /// If not in a transitional state this list is supposed to be empty
    fn get_body_missing_anticone(&self) -> Vec<BlockHash> {
        self.pruning_meta_stores.read().get_body_missing_anticone()
    }

    fn clear_body_missing_anticone_set(&self) {
        let mut pruning_meta_write = self.pruning_meta_stores.write();
        let mut batch = rocksdb::WriteBatch::default();
        pruning_meta_write.set_body_missing_anticone(&mut batch, vec![]).unwrap();
        self.db.write(batch).unwrap();
    }

    fn pruning_point(&self) -> BlockHash {
        self.pruning_point_store.read().pruning_point().unwrap()
    }

    fn create_block_locator_from_pruning_point(&self, high: BlockHash, limit: usize) -> ConsensusResult<Vec<BlockHash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(high)?;
        // Keep the pruning point read guard throughout building the locator
        let pruning_point_read = self.pruning_point_store.read();
        let pruning_point = pruning_point_read.pruning_point().unwrap();
        Ok(self.services.sync_manager.create_block_locator_from_pruning_point(high, pruning_point, Some(limit))?)
    }

    fn estimate_network_hashes_per_second(&self, start_hash: Option<BlockHash>, window_size: usize) -> ConsensusResult<u64> {
        let _guard = self.pruning_lock.blocking_read();
        match start_hash {
            Some(hash) => {
                self.validate_block_exists(hash)?;
                let ghostdag_data = self.ghostdag_store.get_data(hash).unwrap();
                // The selected parent header is used within to check for sampling activation, so we verify its existence first
                if !self.headers_store.has(ghostdag_data.selected_parent).unwrap() {
                    return Err(ConsensusError::DifficultyError(DifficultyError::InsufficientWindowData(0)));
                }
                self.estimate_network_hashes_per_second_impl(&ghostdag_data, window_size)
            }
            None => {
                let virtual_state = self.lkg_virtual_state.load();
                self.estimate_network_hashes_per_second_impl(&virtual_state.ghostdag_data, window_size)
            }
        }
    }

    fn are_pruning_points_violating_finality(&self, pp_list: PruningPointsList) -> bool {
        self.virtual_processor.are_pruning_points_violating_finality(pp_list)
    }

    fn creation_timestamp(&self) -> u64 {
        self.creation_timestamp
    }

    fn finality_point(&self) -> BlockHash {
        self.virtual_processor.virtual_finality_point(&self.lkg_virtual_state.load().ghostdag_data, self.pruning_point())
    }

    /// The utxoset is an additive structure,
    /// to make room for the gradual aggregation of a new utxoset,
    /// first the old one must be cleared.
    /// Likewise, clearing the old utxoset is also a gradual process.
    /// The utxo stable flag guarantees that a full utxoset is never mistaken for
    /// an incomplete or partially deleted one.
    fn clear_pruning_utxo_set(&self) {
        let mut pruning_meta_write = self.pruning_meta_stores.write();
        let mut batch = rocksdb::WriteBatch::default();
        // Currently under the conditions in which this function is called, this flag should already be false.
        // We lower it down regardless as it is conceptually true to do so.
        pruning_meta_write.set_pruning_utxoset_stable_flag(&mut batch, false).unwrap();
        self.db.write(batch).unwrap();
        pruning_meta_write.utxo_set.clear().unwrap();
    }

    /// The usual flow consists of the pruning point naturally updating during pruning, and hence maintains consistency by default
    /// During pruning catchup, we need to manually update the pruning point and
    /// make sure that consensus looks "as if" it has just moved to a new pruning point.
    fn intrusive_pruning_point_update(&self, new_pruning_point: BlockHash, syncer_sink: BlockHash) -> ConsensusResult<()> {
        let pruning_points_to_add = self.get_and_verify_path_to_new_pruning_point(new_pruning_point, syncer_sink)?;

        // If all has gone well, we can finally update pruning point and other stores.
        self.intrusive_pruning_point_store_writes(new_pruning_point, syncer_sink, pruning_points_to_add)
    }

    fn set_pruning_utxoset_stable_flag(&self, val: bool) {
        let mut pruning_meta_write = self.pruning_meta_stores.write();
        let mut batch = rocksdb::WriteBatch::default();

        pruning_meta_write.set_pruning_utxoset_stable_flag(&mut batch, val).unwrap();
        self.db.write(batch).unwrap();
    }

    fn is_pruning_utxoset_stable(&self) -> bool {
        let pruning_meta_read = self.pruning_meta_stores.read();
        pruning_meta_read.pruning_utxoset_stable_flag()
    }

    fn is_pruning_point_anticone_fully_synced(&self) -> bool {
        let pruning_meta_read = self.pruning_meta_stores.read();
        pruning_meta_read.is_anticone_fully_synced()
    }

    fn run_evm_retention_pass(&self, now_ms: u64) -> kaspa_consensus_core::evm::EvmRetentionReport {
        use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader};
        use crate::processes::evm::pruner::{EvmPruneStores, PruneContext, evm_tx_hash_fn, plan_pass};
        use kaspa_consensus_core::evm::{EvmRetentionIdleReason, EvmRetentionReport};

        // Inert lane: nothing to retain, and saying so beats reporting a zero that
        // looks like a stuck pruner.
        if self.config.evm_activation_daa_score == u64::MAX {
            return EvmRetentionReport { idle_reason: EvmRetentionIdleReason::LaneInert, ..Default::default() };
        }
        let policy = self.config.evm_retention_policy;

        // ABSENT is not BROKEN. On a fresh database the canonical-heads singleton has
        // simply never been written, which is the normal state of a node that has not
        // reached the EVM lane yet — reporting that as a store fault sends an operator
        // looking for corruption on a healthy node. Only a real read error is a fault.
        // (This is the same conflation the idle-reason split was introduced to fix,
        // one layer down; live-net running found it because the fault variant is
        // correctly NOT deduplicated and so repeated every tick.)
        let head = match self.storage.evm_heads_store.read().get() {
            Ok(heads) => heads.latest,
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => {
                return EvmRetentionReport { idle_reason: EvmRetentionIdleReason::NoEvmHeadYet, ..Default::default() };
            }
            Err(_) => return EvmRetentionReport { idle_reason: EvmRetentionIdleReason::StoreUnavailable, ..Default::default() },
        };
        let head_evm_number = match self.storage.evm_header_store.get(head) {
            Ok(h) => h.evm_number,
            // A head with no EVM header is pre-activation: the lane is active on this
            // network but this node has not produced an EVM block yet.
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => {
                return EvmRetentionReport { idle_reason: EvmRetentionIdleReason::NoEvmHeadYet, ..Default::default() };
            }
            Err(_) => return EvmRetentionReport { idle_reason: EvmRetentionIdleReason::StoreUnavailable, ..Default::default() },
        };

        // The EVM number of the L1 pruning point bounds the state-history segments.
        // Absent (a block with no EVM header) means "do not touch state history",
        // which `plan_segment` already treats as no plan.
        let l1_pruning_evm_number = {
            let pruning_point = self.storage.pruning_point_store.read().pruning_point().ok();
            pruning_point.and_then(|p| self.storage.evm_header_store.get(p).ok()).map(|h| h.evm_number)
        };

        let ctx = PruneContext {
            head_evm_number,
            // Measured, not assumed: expressing retention in time is pointless if
            // the conversion back to blocks hard-codes a rate.
            blocks_per_second: self.observed_evm_blocks_per_second(head_evm_number),
            consensus_transitional: self.is_consensus_in_transitional_ibd_state(),
            l1_pruning_evm_number,
            batch_rows: policy.batch_rows,
        };

        let cursors = self.storage.evm_prune_cursor_store.clone();
        let plans = plan_pass(&policy, &ctx, |segment| cursors.get(segment).unwrap_or_default());
        if plans.is_empty() {
            return EvmRetentionReport::default();
        }

        let stores = EvmPruneStores {
            db: self.db.clone(),
            cursors,
            numbers: self.storage.evm_number_store.clone(),
            receipts: self.storage.evm_receipts_store.clone(),
            log_index: self.storage.evm_log_index_store.clone(),
            tx_index: self.storage.evm_tx_index_store.clone(),
            raw_tx: self.storage.evm_raw_tx_store.clone(),
            raw_tx_owners: self.storage.evm_raw_tx_owners_store.clone(),
            payloads: self.storage.evm_payload_store.clone(),
            block_hash_map: self.storage.evm_block_hash_map_store.clone(),
            traces: self.storage.evm_trace_store.clone(),
            diffs: self.storage.evm_state_diff_store.clone(),
            anchors_v1: self.storage.evm_state_checkpoint_store.clone(),
            anchors_v2: self.storage.evm_state_checkpoint_v2_store.clone(),
            state_roots: self.storage.evm_block_state_root_store.clone(),
            tx_hash: evm_tx_hash_fn(),
        };

        // Code GC rides the same pass. It runs only when the state segments are
        // eligible: a mark set computed while state history is still being written
        // behind a deferred pruning point would be a snapshot of a moving target,
        // and the cost of being wrong here is deleted bytecode.
        let mut report = EvmRetentionReport::default();
        if !ctx.consensus_transitional && policy.role != kaspa_consensus_core::evm::EvmNodeRole::ArchiveIndexer {
            report.rows_deleted += self.run_evm_code_gc(now_ms);
        }
        for plan in plans {
            match stores.execute(plan, now_ms) {
                Ok(outcome) => {
                    report.rows_deleted += outcome.rows_deleted;
                    report.segments_advanced += 1;
                }
                // A failing segment must not abort the pass: the others are
                // independent, and stalling all retention on one store error is how
                // a recoverable fault becomes a full disk.
                Err(e) => warn!("[evm-retention] segment {} failed: {e}", plan.segment.as_str()),
            }
        }
        report
    }

    fn evm_retention_availability(&self) -> Vec<(kaspa_consensus_core::evm::EvmPruneSegment, u64)> {
        kaspa_consensus_core::evm::EvmPruneSegment::ALL
            .iter()
            .map(|&s| (s, self.storage.evm_prune_cursor_store.get(s).map(|c| c.available_from).unwrap_or(0)))
            .collect()
    }

    fn is_consensus_in_transitional_ibd_state(&self) -> bool {
        let pruning_meta_read = self.pruning_meta_stores.read();
        pruning_meta_read.is_in_transitional_ibd_state()
    }

    fn get_n_last_pruning_points(&self, n: usize) -> Vec<BlockHash> {
        let (_pruning_point, pruning_index) = self.pruning_point_store.read().pruning_point_and_index().unwrap();
        (0..=pruning_index).rev().take(n).map(|ind| self.past_pruning_points_store.get(ind).unwrap()).collect_vec()
    }
}
