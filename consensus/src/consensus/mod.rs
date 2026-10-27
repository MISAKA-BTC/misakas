pub mod cache_policy_builder;
pub mod ctl;
pub mod factory;
pub mod services;
pub mod storage;
pub mod test_consensus;

mod palw_material_hold;
mod evm_replay_market_owned_v2;
#[cfg(test)]
mod evm_view_projection_hook_v2;
mod utxo_set_override;

use kaspa_consensus_core::Hash64;

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
            utxo_set::UtxoSetStoreReader,
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
        ActiveValidatorSet, AttestationQualityDeficit, CanonicalLaggedEpochAnchor, DnsConfirmation, StakeBondPage, StakeBondQuery,
        StakeBondRecord, ValidatorAttestationTarget, ValidatorRecord, dns_confirmation_from_state, epoch_meets_quality_floor,
        is_bond_active_at, paginate_stake_bonds, ready_epoch_from_tip_blue_score, required_stake_for_quality_floor,
        stake_attestation_message,
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
    network::NetworkType,
    pruning::{
        PruningImportSessionStateV1, PruningImportSessionV1, PruningPointProof, PruningPointTrustedData, PruningPointsList,
        PruningProofMetadata,
    },
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
    pub fn new(
        db: Arc<DB>,
        config: Arc<Config>,
        pruning_lock: SessionLock,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
        tx_script_cache_counters: Arc<TxScriptCacheCounters>,
        creation_timestamp: u64,
        mining_rules: Arc<MiningRules>,
    ) -> Result<Self, kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
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
            config.evm_history_mode,                // §12: gate the archive diff/checkpoint writer
            config.evm_shadow_state_backend,        // C-01 S4: node-local shadow dual-write + differential
            config.evm_flat_authoritative,          // C-01 S9: flat-authoritative executor seed
            config.evm_retire_206,                  // C-01 S9b: stop persisting the per-block 206 snapshot
            config.evm_bridge_finality_effective(), // ADR-0109 Decision 2: Label (default) or Pause
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

        // Does this database already carry a chain? `past_pruning_points[0]` is the pruning-proof
        // anchor and is never pruned, so its presence separates "a node resuming its own history"
        // from "a fresh or staging database".
        //
        // **Read BEFORE `virtual_processor.init()`, which is what writes it.** `init` registers a
        // pruning point on any database that has none — `past_pruning_points_store.insert(0,
        // genesis)` — so asking afterwards returns true for every consensus ever constructed,
        // including a staging one three lines old. The guard below then has no way to recognise
        // the single case its own comment calls legitimate, and a `ConsensusV2` node that reached
        // `IbdType::DownloadHeadersProof` killed itself on the staging consensus it had just
        // created. Measured on testnet-11 on 2026-08-26: three fresh joins, three identical
        // panics, one second into IBD — a newcomer could not join the network at all.
        let db_has_history = storage.past_pruning_points_store.get(0).is_ok();

        // Ensure the relations stores are initialized
        header_processor.init();
        // Ensure that some pruning point is registered
        virtual_processor.init();

        // Ensure that genesis was processed
        if config.process_genesis {
            header_processor.process_genesis();
            body_processor.process_genesis();
            virtual_processor.process_genesis()?;
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

        // **A ConsensusV2 node with no PALW state must not run** (launch blockers §1).
        //
        // `PalwChainStateV2` is written only by `process_genesis`. Absent state was read as "no
        // policy", and every PALW authority then failed OPEN: `palw_state_root` unchecked, no
        // transition applied, tips ordered by blue work alone, any pruning point allowed, the
        // deep-reorg comparator skipped, any staged chain accepted at IBD. It does not fork — it is
        // strictly MORE permissive — so nothing warned anyone, and such a node would follow the
        // blue-work-heaviest chain that the frontier-first ordering exists to refuse.
        //
        // Four ways to get there, and only the first is exotic: joining by pruned IBD; starting on
        // a datadir that predates the bundle (bundle-free and bundled testnet-12 share a genesis,
        // so the re-genesis guard above cannot see the difference); a schema-version bump, which
        // `reindex_if_stale` answers by deleting the tip with nothing that rebuilds it; and the
        // staging consensus, built with `.skip_adding_genesis()`.
        //
        // Refusing is not the whole fix — a node that CANNOT install the state still cannot join,
        // which is what the pruning-point import is for — but it converts a silent consensus
        // divergence into a startup message, and that is the half that must never be missing.
        //
        // **It must cover the node that did NOT process genesis**, because that is exactly where
        // the danger is: the pruned join and the pre-bundle datadir never process genesis, which is
        // WHY they have no state. Gating on `process_genesis` alone would have excluded them.
        //
        // The one legitimate stateless case is a STAGING consensus — built with
        // `.skip_adding_genesis()` on a fresh database, holding nothing until it is promoted. It is
        // told apart by its database, not by a flag: staging has no history, while a node resuming
        // its own chain has `past_pruning_points[0]`. So the guard runs when this consensus either
        // installed genesis itself or is resuming a real history — and `db_has_history` is read
        // above `init`, because `init` would otherwise hand staging the very row that distinguishes
        // it. A staging consensus never survives a restart (`delete_inactive_consensus_entries`
        // drops the entry at startup), so "fresh" and "staging" are the same database twice over.
        if (config.process_genesis || db_has_history)
            && matches!(config.params.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_))
        {
            let installed = storage.palw_state_v2_store.read().tip_record().ok().flatten().is_some();
            assert!(
                installed,
                "this is a PALW ConsensusV2 network ({}) and the PALW state store holds no tip — every PALW consensus rule \
                 would be silently disabled on this node (state root unchecked, tips ordered by blue work, any pruning point \
                 accepted) and it would NOT fork, so nothing would report it. Wipe the data directory to re-genesis, or wait \
                 for a build that can import the state at a pruning point",
                config.params.net
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

        Ok(this)
    }

    /// A procedure for calling database upgrades which are self-contained (i.e., do not require knowing the DB version)
    fn run_database_upgrades(&self) {
        // Upgrade to initialize the new retention root field correctly
        self.retention_root_database_upgrade();
        self.consensus_transitional_flags_upgrade();
        // C-01 S9b-prune: one-shot bulk reclamation of the legacy 206 snapshot store (opt-in, gated).
        self.evm_legacy_206_bulk_prune();
        // F2c (t10 recovery): one-shot backfill of the pruning-point EVM anchor (opt-in, verified).
        self.evm_materialize_pp_anchor_one_shot();
    }

    /// F2c (t10 recovery): ONE-SHOT backfill of the pruning point's EVM state anchor, run at startup
    /// when `--evm-materialize-pp-anchor` is set. For a retired-206 node whose datadir predates the
    /// pruning processor's pp-anchor step: its sub-pp §12 rows are pruned, its flat head is far past
    /// the pp, so `pruning_point_evm_state` has no serve path and every peer's pruned IBD dies at the
    /// EVM snapshot stage (the testnet-10 wedge). The §12 diffs for the whole retained window
    /// (pp..head) are still present on a `recent` node and carry `before` views, so the pp state is
    /// recomputed by REVERSE-replaying them from the (root-verified) flat head down to the pp. The
    /// result is verified against the pp's COMMITTED `state_root` before anything is persisted — a
    /// mismatch aborts with nothing written, so a rehearsal on a datadir copy and the production run
    /// enforce the same gate. Idempotent: an existing anchor short-circuits.
    fn evm_materialize_pp_anchor_one_shot(&self) {
        if !self.config.evm_materialize_pp_anchor {
            return;
        }
        #[cfg(not(feature = "evm"))]
        {
            warn!(
                "[F2c] --evm-materialize-pp-anchor requires a kaspad built with --features evm; skipping (no EVM state on this build)."
            );
        }
        #[cfg(feature = "evm")]
        {
            use crate::model::stores::evm::{
                EvmCodeStoreReader, EvmHeaderStoreReader, EvmStateCheckpointStoreReader, EvmStateDiffStoreReader, EvmStateStoreReader,
            };

            let pp = match self.pruning_point_store.read().pruning_point() {
                Ok(pp) => pp,
                Err(e) => {
                    warn!("[F2c] cannot read the pruning point ({e}); skipping the anchor backfill this startup.");
                    return;
                }
            };
            let pp_header = match self.storage.evm_header_store.get(pp) {
                Ok(h) => h,
                Err(_) => {
                    info!("[F2c] pruning point {pp} has no committed EVM header (pre-activation chain segment); nothing to anchor.");
                    return;
                }
            };
            if self.storage.evm_state_checkpoint_store.get(pp).ok().flatten().is_some() || self.storage.evm_state_store.get(pp).is_ok()
            {
                info!("[F2c] pruning point {pp} already has a state anchor (checkpoint or 206 row); nothing to do.");
                return;
            }
            // The start of the reverse replay is the flat head — verify it is FAITHFUL
            // (its on-disk rows keccak-MPT-hash to the committed head root) before
            // trusting a single reverse step, mirroring the S9b-prune gold standard.
            let flat_ptr = match self.storage.evm_latest_state_ptr_store.read().get() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    warn!(
                        "[F2c] the flat state pointer is absent — no start state to reverse-replay from. Aborting (nothing written)."
                    );
                    return;
                }
                Err(e) => {
                    warn!("[F2c] flat state pointer read failed ({e}); aborting (nothing written).");
                    return;
                }
            };
            let start_snapshot = match crate::processes::evm::materialize_snapshot(
                &self.storage.evm_flat_account_store,
                &self.storage.evm_code_store,
            ) {
                Ok(s) => s,
                Err(e) => {
                    warn!("[F2c] flat materialization failed ({e}); aborting (nothing written).");
                    return;
                }
            };
            let head_committed_root = match self.storage.evm_header_store.get(flat_ptr.canonical_head) {
                Ok(h) => h.state_root,
                Err(e) => {
                    warn!(
                        "[F2c] cannot read the committed EVM header of the flat head {} ({e}); aborting (nothing written).",
                        flat_ptr.canonical_head
                    );
                    return;
                }
            };
            let root_of = |snap: &kaspa_consensus_core::evm::EvmStateSnapshot| {
                kaspa_evm::snapshot::seed_cachedb(snap)
                    .map_err(|e| e.to_string())
                    .map(|cdb| kaspa_hashes::EvmH256::from_bytes(kaspa_evm::state::state_root(&cdb).0))
            };
            match root_of(&start_snapshot) {
                Ok(r) if r == head_committed_root => {}
                Ok(r) => {
                    warn!(
                        "[F2c] the flat head {} is NOT faithful (rows hash to {r:?}, committed {head_committed_root:?}); \
                         refusing to reverse-replay from a wrong start. Aborting (nothing written).",
                        flat_ptr.canonical_head
                    );
                    return;
                }
                Err(e) => {
                    warn!("[F2c] could not hash the flat head state ({e}); aborting (nothing written).");
                    return;
                }
            }
            info!(
                "[F2c] reverse-replaying §12 diffs from the flat head {} down to the pruning point {pp} — this walks the whole retained window once and may take a while...",
                flat_ptr.canonical_head
            );
            let snapshot = if flat_ptr.canonical_head == pp {
                start_snapshot
            } else {
                match crate::processes::evm::reverse_replay_to_block(
                    start_snapshot,
                    flat_ptr.canonical_head,
                    pp,
                    |b| self.storage.evm_state_diff_store.get(b),
                    |h| self.storage.evm_code_store.get(*h).ok().flatten(),
                    |steps| {
                        if steps % 100_000 == 0 {
                            info!("[F2c] reverse replay progress: {steps} diffs applied");
                        }
                    },
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("[F2c] reverse replay failed: {e}. Aborting (nothing written).");
                        return;
                    }
                }
            };
            // The gate the whole feature hangs on: the recomputed pp state MUST hash
            // to the pp's committed state_root, or nothing is persisted.
            match root_of(&snapshot) {
                Ok(r) if r == pp_header.state_root => {}
                Ok(r) => {
                    warn!(
                        "[F2c] the reverse-replayed pruning-point state hashes to {r:?} but the committed state_root is {:?}; \
                         REFUSING to persist an unverified anchor. Aborting (nothing written).",
                        pp_header.state_root
                    );
                    return;
                }
                Err(e) => {
                    warn!("[F2c] could not hash the reverse-replayed state ({e}); aborting (nothing written).");
                    return;
                }
            }
            let checkpoint =
                kaspa_consensus_core::evm::EvmStateCheckpointV1::build(pp, pp_header.evm_number, pp_header.state_root, &snapshot);
            let mut batch = rocksdb::WriteBatch::default();
            self.storage.evm_state_checkpoint_store.insert_batch(&mut batch, pp, checkpoint).unwrap();
            self.db.write(batch).unwrap();
            info!(
                "[F2c] pruning-point EVM anchor materialized and VERIFIED for {pp} (evm number {}); pruned-IBD serving is restored. \
                 Drop --evm-materialize-pp-anchor on the next restart (a present anchor makes it a no-op).",
                pp_header.evm_number
            );
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
        if match self.statuses_store.read().get(hash).optional()
            .map_err(|e| ConsensusError::GeneralOwned(format!("native block status read: {e}")))? {
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
        // Resetting virtual, including at the SAME point, invalidates a prior
        // stream's completion. Serialize it with begin and every import mutation.
        let _import_mutation = self.virtual_processor.pruning_import_session_lock.lock();
        // All fallible traversal/session reads precede writers that stage caches.
        let mut anticone = self.services.dag_traversal_manager.anticone(new_pruning_point, [syncer_sink].into_iter(), None)?;
        anticone.push(new_pruning_point);
        let mut import = self
            .pruning_meta_stores
            .read()
            .import_session()
            .map_err(|e| ConsensusError::GeneralOwned(format!("reading pruning import authority: {e}")))?;
        if import.epoch > 0 {
            import.epoch = import.epoch.checked_add(1).ok_or(ConsensusError::General("pruning import epoch exhausted"))?;
        }
        import.active = None;
        import.utxos_started = false;
        import.utxos_imported = false;
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
        {
            let mut meta = self.pruning_meta_stores.write();
            meta.set_pruning_utxoset_stable_flag(&mut batch, false).unwrap();
            meta.set_import_session(&mut batch, &import).unwrap();
            // Store the currently bodyless anticone for trusted validation.
            meta.set_body_missing_anticone(&mut batch, anticone).unwrap();
        }
        self.virtual_processor.write_pruning_import_barrier(batch);
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
    /// lagged anchor — the exact `(net_id, epoch, target_hash, target_daa_score, vsc,
    /// bond)` digest the v3 verifier reconstructs (`collect_stake_contributions_v2`). The
    /// service only signs `message`. Shared by the singular + batch signers.
    ///
    /// The VSC is the audit-#4 fixed zero (P-1D: ADR-0017 retired the committee; kept for domain
    /// separation), the only value the acceptance gate admits.
    fn build_attestation_target(
        &self,
        anchor: &CanonicalLaggedEpochAnchor,
        bond_outpoint: TransactionOutpoint,
    ) -> ValidatorAttestationTarget {
        let vsc = kaspa_hashes::Hash64::default();
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

    /// The PALW V2 state at the tip, through the shared materialization (audit M-7) — the one
    /// snapshot every `palw_model_*` reader answers from. `None` off ConsensusV2 or before the
    /// first state was written.
    pub(crate) fn palw_state_v2_tip(&self) -> Option<Arc<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1>> {
        let state_params = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => &bundle.state,
            _ => return None,
        };
        let (_chain_point, state) = self.virtual_processor.palw_v2_reader_state(state_params).ok().flatten()?;
        Some(state)
    }

    /// ADR-0088 Decision 12: one line's row for a reader — written or synthesised — with the
    /// payout payloads of the bonds its roles name, each read from the bond registry at the same
    /// tip. A role naming a bond the registry no longer holds reads as no payload, never as an
    /// error: the row is the chain's, the lookup is the reader's convenience.
    fn palw_model_line_row_read_v1(
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        line_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::api::PalwModelLineRowReadV1> {
        let line = state.model_line_or_founding(&line_id)?;
        let payload = |bond: Option<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>| {
            bond.and_then(|b| state.bond(&b)).map(|record| record.payout_payload)
        };
        Some(kaspa_consensus_core::api::PalwModelLineRowReadV1 {
            line_id,
            has_row: state.model_line(&line_id).is_some(),
            owner_payout_payload: payload(line.owner),
            developer_payout_payload: payload(line.developer),
            maintainer_payout_payload: payload(line.maintainer),
            line,
        })
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

    fn heartbeat_adapt_block_template(&self, template: BlockTemplate) -> Result<(BlockTemplate, u64), RuleError> {
        self.virtual_processor.heartbeat_adapt_block_template(template)
    }

    fn heartbeat_yield_hint(&self) -> kaspa_consensus_core::palw_heartbeat_v1::HeartbeatYieldHintV1 {
        self.virtual_processor.heartbeat_yield_hint()
    }

    fn palw_round_view_v1(&self, round: u64) -> Option<kaspa_consensus_core::palw_execution_lane_v1::PalwExecRoundViewV1> {
        self.virtual_processor.palw_round_view_v1(round)
    }

    fn palw_round_lane_status_v1(&self, round: u64) -> Option<kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneStatusV1> {
        self.virtual_processor.palw_round_lane_status_v1(round)
    }

    fn palw_recent_executions_v1(
        &self,
        limit: usize,
    ) -> Option<(Vec<kaspa_consensus_core::palw_exec_view_v1::PalwExecutionRowV1>, usize)> {
        self.virtual_processor.palw_recent_executions_v1(limit)
    }

    fn palw_block_lane_v1(&self, hash: BlockHash) -> Option<kaspa_consensus_core::palw_exec_view_v1::PalwBlockLaneV1> {
        use kaspa_consensus_core::palw_exec_view_v1::{
            PalwBlockClassV1, PalwBlockLaneV1, palw_round_block_class_v1, palw_round_lane_telemetry_v1,
        };
        let header = self.headers_store.get_header(hash).ok()?;
        if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1
            && self.config.params.palw_execution_lane_fence().is_some()
        {
            let envelope = kaspa_consensus_core::palw_execution_lane_v1::PalwExecEnvelopeV1::decode(&header.palw_commitment)
                .ok()
                .map(|e| (e.round, e.permit_index, e.bond));
            let record = palw_round_lane_telemetry_v1().lookup(&hash);
            return Some(PalwBlockLaneV1 { class: palw_round_block_class_v1(record.as_ref()), envelope, record });
        }
        let class = if self.is_chain_block(hash).unwrap_or(false) {
            PalwBlockClassV1::Blue
        } else {
            match self.get_current_block_color(hash) {
                Some(true) => PalwBlockClassV1::Blue,
                Some(false) => PalwBlockClassV1::Red,
                None => PalwBlockClassV1::Unmerged,
            }
        };
        Some(PalwBlockLaneV1 { class, envelope: None, record: None })
    }

    /// ADR-0127 Decision 3, on the node: the tip state every `palw_*` read takes, the rules it was
    /// folded under, and the DAA score of the frontier's carrying header — which the read dates the
    /// frontier by only once the frontier's claim has retired from that state.
    fn palw_settlement_v1(&self, daa_score: u64) -> Option<kaspa_consensus_core::palw_settlement_v1::PalwSettlementV1> {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return None;
        };
        let state = self.palw_state_v2_tip()?;
        let (_, frontier) = state.safe_frontier();
        let frontier_header_daa = self.headers_store.get_daa_score(frontier).ok();
        kaspa_consensus_core::palw_settlement_v1::palw_settlement_v1(&state, &bundle.state, frontier_header_daa, daa_score)
    }

    fn round_adapt_block_template(
        &self,
        template: BlockTemplate,
        round: u64,
        payout: kaspa_consensus_core::tx::ScriptPublicKey,
    ) -> Result<BlockTemplate, RuleError> {
        self.virtual_processor.round_adapt_block_template(template, round, payout)
    }

    fn exec_v2_slice_adapt_block_template(
        &self,
        template: BlockTemplate,
        payout: kaspa_consensus_core::tx::ScriptPublicKey,
    ) -> Result<BlockTemplate, RuleError> {
        self.virtual_processor.exec_v2_slice_adapt_block_template(template, payout)
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

    fn palw_producer_facts_v2(
        &self,
        class_id: kaspa_hashes::Hash64,
        bond: Option<kaspa_consensus_core::tx::TransactionOutpoint>,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2> {
        self.virtual_processor.palw_producer_facts_v2_impl(class_id, bond)
    }

    fn palw_fp_commitment_price_v1(
        &self,
        class_id: kaspa_hashes::Hash64,
        prompt_token_ids: Vec<u32>,
        prompt_tokens: u32,
        decode_tokens_executed: u32,
        work_leaves: u64,
        bond: Option<kaspa_consensus_core::tx::TransactionOutpoint>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwFpPriceAnswerV1> {
        self.virtual_processor.palw_fp_commitment_price_impl(
            class_id,
            &prompt_token_ids,
            prompt_tokens,
            decode_tokens_executed,
            work_leaves,
            bond,
        )
    }

    fn palw_fp_commitment_price_answered_v1(
        &self, commitment: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        prompt_token_ids: &[u32], answer: &[u32],
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwFpPriceAnswerV1> {
        self.virtual_processor.palw_fp_commitment_price_answered_impl(commitment, prompt_token_ids, answer)
    }

    fn palw_fp_spendable_v3(
        &self,
        bond: kaspa_consensus_core::tx::TransactionOutpoint,
    ) -> Vec<kaspa_consensus_core::palw_freeprompt_v3::PalwFpSpendableQuantumV3> {
        self.virtual_processor.palw_fp_spendable_v3_impl(bond)
    }

    fn palw_locked_bond_outpoints_v2(&self) -> Vec<kaspa_consensus_core::tx::TransactionOutpoint> {
        self.virtual_processor.palw_locked_bond_outpoints_v2_impl()
    }

    fn palw_partition_refusal_v1(&self) -> Option<kaspa_consensus_core::api::PalwPartitionRefusalV1> {
        self.virtual_processor.palw_partition_refusal_v1()
    }

    /// ADR-0078 Decision 5, on the node: the materialized tip, then [`palw_derived_claim_view_v1`].
    ///
    /// The state store's tip is a snapshot read like every other `palw_*` answer here, and the
    /// arithmetic is the free function below so a test can put the same question to a folded state
    /// without a store — the read a consumer makes and the read the transition just wrote must be
    /// the same read.
    #[allow(clippy::type_complexity)]
    fn palw_derived_artifacts_v1(
        &self,
        claim_id: kaspa_hashes::Hash64,
    ) -> Option<(
        kaspa_consensus_core::palw_state_v2::PalwClaimStateV2,
        Vec<u8>,
        Vec<(kaspa_consensus_core::palw_derived_v1::PalwDerivedKeyV1, kaspa_consensus_core::palw_derived_v1::PalwDerivedRowV1)>,
    )> {
        let state_params = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => &bundle.state,
            _ => return None,
        };
        // Audit M-7: the shared materialization, so a read path cannot cost the node a whole
        // re-rooting of the PALW state per unauthenticated call.
        let (_chain_point, state) = self.virtual_processor.palw_v2_reader_state(state_params).ok().flatten()?;
        palw_derived_claim_view_v1(&state, claim_id)
    }

    fn palw_material_income_hold_until_v1(&self, claim: kaspa_hashes::Hash64) -> Option<u64> {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return None;
        };
        let before = self.lkg_virtual_state.load();
        let active = self.config.params.palw_material_income_active_at(before.daa_score);
        if !active {
            return None;
        }
        let complete = || {
            self.virtual_processor
                .pruning_import_status()
                .ok()
                .and_then(|(session, stable)| (stable && session.active.is_none()).then_some(session.epoch))
        };
        let import_before = complete();
        let snapshot = self.virtual_processor.palw_v2_reader_state(&bundle.state);
        let after = self.lkg_virtual_state.load();
        // The general reader may return its old tip when reconstruction fails.
        // That fallback, an import, or a changing generation cannot authorize
        // irreversible file deletion. Preserve a healthy snapshot's real None.
        palw_material_hold::material_hold_from_snapshot_v1(
            active,
            import_before.is_some()
                && import_before == complete()
                && before.ghostdag_data.selected_parent == after.ghostdag_data.selected_parent,
            after.ghostdag_data.selected_parent,
            snapshot,
            |state| state.material_income_hold_until_v1(&bundle.state, claim),
        )
    }

    fn palw_native_material_provider_authority_v3(
        &self,
        checkpoint: BlockHash,
        header: &Header,
        ready: &kaspa_consensus_core::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3,
    ) -> Result<kaspa_consensus_core::palw_attempt_material_v3::PalwNativeMaterialProviderAuthorityV3, String> {
        self.virtual_processor.palw_native_material_provider_authority_v3_impl(checkpoint, header, ready)
    }
    fn palw_native_material_provider_authority_v4(
        &self,
        checkpoint: BlockHash,
        header: &Header,
        ready: &kaspa_consensus_core::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3,
    ) -> Result<kaspa_consensus_core::api::PalwNativeMaterialProviderAuthorityV4, String> {
        self.virtual_processor.palw_native_material_provider_authority_v4_impl(&self.config.params, checkpoint, header, ready)
    }
    fn palw_native_routing_execution_authority_v2(
        &self,
        checkpoint: BlockHash,
        candidate: &Header,
        job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
        executor: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingExecutionAuthorityV2>>, String> {
        self.virtual_processor.palw_native_routing_execution_authority_v2_impl(
            &self.config.params,
            checkpoint,
            candidate,
            job,
            executor,
        )
    }
    fn palw_native_routing_fp_work_authority_v2(
        &self, fp: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3, job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
    ) -> Result<Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>>, kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> { self.virtual_processor.palw_native_routing_fp_work_authority_v2_impl(&self.config.params,fp,job) }
    fn palw_native_routing_rider_work_authority_v2(
        &self, job: &kaspa_consensus_core::palw_v2::PalwJobContextV2, executor: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        lead: kaspa_consensus_core::Hash64, index: u32, retention: u64,
    ) -> Result<Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>>, kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> { self.virtual_processor.palw_native_routing_rider_work_authority_v2_impl(&self.config.params,job,executor,lead,index,retention) }
    fn palw_native_routing_refresh_work_authority_v2(
        &self, original: &kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2,
    ) -> Result<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>, kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> { self.virtual_processor.palw_native_routing_refresh_work_authority_v2_impl(&self.config.params,original) }
    fn palw_native_routing_claim_authority_v2(
        &self,
        claim: Hash64,
        job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
    ) -> Result<Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingClaimAuthorityV2>>, String> {
        self.virtual_processor.palw_native_routing_claim_authority_v2_impl(&self.config.params, claim, job)
    }
    fn palw_paid_material_generation_snapshot_v5(
        &self,
        original: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<kaspa_consensus_core::palw_paid_material_generation_v5::PalwPaidMaterialGenerationSnapshotV5, String> {
        use kaspa_consensus_core::{
            palw_paid_claim_observation_v5::NativePaidClaimSnapshotV5,
            palw_paid_material_generation_v5::{PalwPaidMaterialGenerationSnapshotV5, paid_material_generation_for_original_v5},
        };
        if self.config.params.palw_claim_challenge_v2.is_none() || self.config.params.palw_material_release_recovery_v2.is_none() {
            return Err("own paid generation requires explicit paid/recovery capabilities".into());
        }
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("own paid generation requires ConsensusV2".into());
        };
        let before = self.lkg_virtual_state.load();
        let (import, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || import.active.is_some() {
            return Err("own paid generation import incomplete".into());
        }
        let (checkpoint, state) = self
            .virtual_processor
            .palw_v2_reader_state(&bundle.state)
            .map_err(|e| e.to_string())?
            .ok_or("own paid generation state absent")?;
        if checkpoint != before.ghostdag_data.selected_parent || state.last_point().is_none_or(|p| p.block != checkpoint) {
            return Err("own paid generation point differs from selected parent".into());
        }
        let generation = paid_material_generation_for_original_v5(&state, original).map_err(|e| e.to_string())?;
        let result = PalwPaidMaterialGenerationSnapshotV5 {
            snapshot: NativePaidClaimSnapshotV5 {
                checkpoint,
                state_point: checkpoint,
                state_root: self.virtual_processor.palw_committed_root_v1(&state),
                daa: before.daa_score,
                import_epoch: import.epoch,
            },
            generation,
        };
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || end.epoch != import.epoch
            || checkpoint != after.ghostdag_data.selected_parent
            || before.daa_score != after.daa_score
        {
            return Err("own paid generation changed point/clock/import generation".into());
        }
        Ok(result)
    }
    fn palw_paid_claim_observation_v5(
        &self,
        base: kaspa_hashes::Hash64,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<
            kaspa_consensus_core::palw_paid_claim_observation_v5::NativePaidClaimObservationV5,
        >,
        String,
    > {
        use kaspa_consensus_core::palw_paid_claim_observation_v5::{NativePaidClaimSnapshotV5, native_paid_claim_observation_v5};
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("native paid observations require ConsensusV2".into());
        };
        let before = self.lkg_virtual_state.load();
        let (import, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || import.active.is_some() {
            return Err("native paid observation import is incomplete".into());
        }
        let (checkpoint, state) = self
            .virtual_processor
            .palw_v2_reader_state(&bundle.state)
            .map_err(|e| e.to_string())?
            .ok_or("native paid observation state absent")?;
        let point = state.last_point().ok_or("native paid observation state point absent")?;
        if checkpoint != before.ghostdag_data.selected_parent || point.block != checkpoint {
            return Err("native paid observation is not the own selected parent".into());
        }
        let snapshot = NativePaidClaimSnapshotV5 {
            checkpoint,
            state_point: point.block,
            state_root: self.virtual_processor.palw_committed_root_v1(&state),
            daa: before.daa_score,
            import_epoch: import.epoch,
        };
        let observed = native_paid_claim_observation_v5(&state, snapshot, base).map_err(|e| e.to_string())?;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || import.epoch != end.epoch
            || checkpoint != after.ghostdag_data.selected_parent
            || before.daa_score != after.daa_score
        {
            return Err("native paid observation changed own point/clock/import generation".into());
        }
        Ok(observed)
    }
    fn palw_paid_bootstrap_observation_v6(
        &self,
        base: Hash64,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<
            kaspa_consensus_core::palw_paid_claim_observation_v6::NativePaidBootstrapObservationV6,
        >,
        String,
    > {
        use kaspa_consensus_core::{
            palw_paid_claim_observation_v5::NativePaidClaimSnapshotV5,
            palw_paid_claim_observation_v6::native_paid_bootstrap_observation_v6,
        };
        let before = self.lkg_virtual_state.load();
        let fence = self
            .config
            .params
            .palw_paid_bootstrap_source_v6
            .filter(|f| {
                f.activation != kaspa_consensus_core::config::params::ForkActivation::never()
                    && f.activation.is_active(before.daa_score)
            })
            .ok_or("own V6 paid observation capability is dormant")?;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("own V6 paid observation requires ConsensusV2".into());
        };
        if !bundle.state.paid_bootstrap_source_v6_mirror_matches(fence.activation.daa_score(), &fence.policy) {
            return Err("own V6 paid observation Params mirror differs".into());
        }
        let (import, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || import.active.is_some() {
            return Err("own V6 paid observation import incomplete".into());
        }
        let (checkpoint, state) = self
            .virtual_processor
            .palw_v2_reader_state(&bundle.state)
            .map_err(|e| e.to_string())?
            .ok_or("own V6 paid observation state absent")?;
        let point = state.last_point().ok_or("own V6 paid observation state point absent")?;
        if checkpoint != before.ghostdag_data.selected_parent || point.block != checkpoint {
            return Err("own V6 paid observation is not the current selected parent".into());
        }
        let observed = native_paid_bootstrap_observation_v6(
            &state,
            NativePaidClaimSnapshotV5 {
                checkpoint,
                state_point: point.block,
                state_root: self.virtual_processor.palw_committed_root_v1(&state),
                daa: before.daa_score,
                import_epoch: import.epoch,
            },
            base,
            fence.policy,
        )
        .map_err(|e| e.to_string())?;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || end.epoch != import.epoch
            || checkpoint != after.ghostdag_data.selected_parent
            || before.daa_score != after.daa_score
        {
            return Err("own V6 paid observation changed point/clock/import generation".into());
        }
        Ok(observed)
    }
    fn palw_source_bootstrap_observation_v1(
        &self,
        work: kaspa_hashes::Hash64,
    ) -> Result<
        kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapObservationV1,
        kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1,
    > {
        use kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1 as F;
        use kaspa_consensus_core::palw_state_v2::PalwStateV2Error as E;
        let observed = self.palw_source_bootstrap_epoch_observation_v1()?;
        let fault = |error: E| match error {
            E::NativeResource(why) => F::Capacity(why),
            E::NativeContextUnavailable(why) => why,
            _ => F::StoreUnreadable,
        };
        if observed.state.source_bootstrap_work_v1(work).map_err(fault)?.is_none() {
            return Err(F::HistoryUnavailable);
        }
        observed.state.source_bootstrap_check_work_authority_v1(work).map_err(fault)?;
        let after = self.lkg_virtual_state.load();
        let import_fault = |error: kaspa_consensus_core::errors::pruning::PruningImportError| {
            use kaspa_consensus_core::errors::pruning::PruningImportError as I;
            match error {
                I::PruningPointImportContextUnavailable(_, _)
                | I::PruningImportSessionRefused { .. }
                | I::ImportedPalwStateHeaderMissing(_)
                | I::MissingPointedPruningPoint
                | I::PruningPointsWrongGenesis(_, _) => F::HistoryUnavailable,
                _ => F::StoreUnreadable,
            }
        };
        let (import, stable) = self.virtual_processor.pruning_import_status().map_err(import_fault)?;
        if !stable
            || import.active.is_some()
            || import.epoch != observed.import_epoch
            || after.ghostdag_data.selected_parent != observed.checkpoint
            || after.daa_score != observed.virtual_daa
        {
            return Err(F::HistoryUnavailable);
        }
        Ok(observed)
    }
    fn palw_source_bootstrap_epoch_observation_v1(
        &self,
    ) -> Result<
        kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapObservationV1,
        kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1,
    > {
        use kaspa_consensus_core::palw_source_bootstrap_v1::{
            PalwSourceBootstrapContextFaultV1 as F, PalwSourceBootstrapObservationV1,
        };
        use kaspa_consensus_core::palw_state_v2::PalwStateV2Error as E;
        let fault = |e: kaspa_database::prelude::StoreError| match e {
            kaspa_database::prelude::StoreError::KeyNotFound(_) => F::HistoryUnavailable,
            kaspa_database::prelude::StoreError::LocalResourceUnavailable(_) => F::CapacityUnavailable,
            _ => F::StoreUnreadable,
        };
        let import_fault = |error: kaspa_consensus_core::errors::pruning::PruningImportError| {
            use kaspa_consensus_core::errors::pruning::PruningImportError as I;
            match error {
                I::PruningPointImportContextUnavailable(_, _)
                | I::PruningImportSessionRefused { .. }
                | I::ImportedPalwStateHeaderMissing(_)
                | I::MissingPointedPruningPoint
                | I::PruningPointsWrongGenesis(_, _) => F::HistoryUnavailable,
                _ => F::StoreUnreadable,
            }
        };
        let state_fault = |e: E| match e {
            E::NativeResource(why) => F::Capacity(why),
            E::NativeContextUnavailable(why) => why,
            _ => F::StoreUnreadable,
        };
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err(F::HistoryUnavailable);
        };
        let before = self.lkg_virtual_state.load();
        let fence = self
            .config
            .params
            .palw_selected_source_bootstrap_v1
            .as_ref()
            .filter(|f| f.at != kaspa_consensus_core::config::params::ForkActivation::never() && f.at.is_active(before.daa_score))
            .ok_or(F::HistoryUnavailable)?;
        let (import, stable) = self.virtual_processor.pruning_import_status().map_err(import_fault)?;
        if !stable || import.active.is_some() {
            return Err(F::HistoryUnavailable);
        }
        let (checkpoint, state) =
            self.virtual_processor.palw_v2_reader_state(&bundle.state).map_err(fault)?.ok_or(F::HistoryUnavailable)?;
        if checkpoint != before.ghostdag_data.selected_parent || state.last_point().is_none_or(|p| p.block != checkpoint) {
            return Err(F::HistoryUnavailable);
        }
        let epoch = state.source_bootstrap_epoch_v1().map_err(state_fault)?.ok_or(F::HistoryUnavailable)?;
        if epoch.context.policy != fence.policy {
            return Err(F::StoreUnreadable);
        }
        state.source_bootstrap_check_epoch_authority_v1().map_err(state_fault)?;
        let after = self.lkg_virtual_state.load();
        let (after_import, stable) = self.virtual_processor.pruning_import_status().map_err(import_fault)?;
        if !stable
            || after_import.active.is_some()
            || import.epoch != after_import.epoch
            || checkpoint != after.ghostdag_data.selected_parent
            || before.daa_score != after.daa_score
        {
            return Err(F::HistoryUnavailable);
        }
        Ok(PalwSourceBootstrapObservationV1 { checkpoint, virtual_daa: before.daa_score, import_epoch: import.epoch, state })
    }
    fn palw_monetary_material_covenants_v1(
        &self,
    ) -> Result<Vec<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1>, String> {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("monetary material requires ConsensusV2".into());
        };
        let before = self.lkg_virtual_state.load();
        let (import, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || import.active.is_some() {
            return Err("monetary material view has an incomplete import".into());
        }
        let (point, state) = self
            .virtual_processor
            .palw_v2_reader_state(&bundle.state)
            .map_err(|e| e.to_string())?
            .ok_or("monetary material state is missing")?;
        let after = self.lkg_virtual_state.load();
        let (after_import, after_stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !after_stable
            || after_import.active.is_some()
            || import.epoch != after_import.epoch
            || point != after.ghostdag_data.selected_parent
            || before.ghostdag_data.selected_parent != point
        {
            return Err("monetary material view changed generation or returned an old tip".into());
        }
        state.assert_monetary_material_consistency_v1(&bundle.state).map_err(|e| e.to_string())?;
        Ok(state.monetary_material_rows_v1())
    }

    fn palw_material_recovery_observation_v4(
        &self,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<
            kaspa_consensus_core::palw_material_recovery_observation_v4::NativeRecoveryObservationV4,
        >,
        String,
    > {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("recovery observation requires ConsensusV2".into());
        };
        let before = self.lkg_virtual_state.load();
        let (import, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || import.active.is_some() {
            return Err("recovery observation import incomplete".into());
        }
        let policy = bundle.state.material_release_recovery_at_v2(before.daa_score).ok_or("recovery observation gate dormant")?;
        let (point, state) = self
            .virtual_processor
            .palw_v2_reader_state(&bundle.state)
            .map_err(|e| e.to_string())?
            .ok_or("recovery observation state absent")?;
        if point != before.ghostdag_data.selected_parent {
            return Err("recovery observation is not the accepted sink".into());
        }
        let finalized = self.palw_finalized_material_recovery_state_v2()?;
        let result = kaspa_consensus_core::palw_material_recovery_observation_v4::native_recovery_observation_v4(
            &state,
            point,
            before.daa_score,
            import.epoch,
            finalized.as_ref().map(|(pin, state)| (*pin, &**state)),
            policy.max_reserved_contracts,
        )?;
        let after = self.lkg_virtual_state.load();
        let (after_import, after_stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !after_stable
            || after_import.active.is_some()
            || after_import.epoch != import.epoch
            || point != after.ghostdag_data.selected_parent
            || before.daa_score != after.daa_score
        {
            return Err("recovery observation changed native/import generation".into());
        }
        Ok(result)
    }

    /// RFC-0008 PAV3: the same coherent tip read as [`Self::palw_monetary_material_covenants_v1`], with the canonical session
    /// selected by the ONE rule `verify_prior` re-runs and the rows/bonds taken from that single state snapshot.
    fn palw_native_session_v3(
        &self,
        attempt: &kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2,
        daa: u64,
    ) -> Result<
        (
            BlockHash,
            Option<(
                kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
                Vec<(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2, kaspa_hashes::Hash64)>,
            )>,
        ),
        String,
    > {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("native session requires ConsensusV2".into());
        };
        let before = self.lkg_virtual_state.load();
        let (import, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || import.active.is_some() {
            return Err("native session view has an incomplete import".into());
        }
        let (point, state) = self
            .virtual_processor
            .palw_v2_reader_state(&bundle.state)
            .map_err(|e| e.to_string())?
            .ok_or("native session state is missing")?;
        let after = self.lkg_virtual_state.load();
        let (after_import, after_stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !after_stable
            || after_import.active.is_some()
            || import.epoch != after_import.epoch
            || point != after.ghostdag_data.selected_parent
            || before.ghostdag_data.selected_parent != point
        {
            return Err("native session view changed generation or returned an old tip".into());
        }
        state.assert_monetary_material_consistency_v1(&bundle.state).map_err(|e| e.to_string())?;
        let operator_of = |provider: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2| state.bond(provider).map(|b| b.operator_id);
        let Some(row) = kaspa_consensus_core::palw_attempt_material_v3::select_native_session_with_recovery_v4(
            &state,
            &bundle.state,
            attempt,
            daa,
        )
        .map_err(|e| e.to_string())?
        else {
            return Ok((point, None));
        };
        let operators = row
            .providers
            .iter()
            .filter(|p| !p.charged)
            .map(|p| operator_of(&p.provider).map(|operator| (p.provider, operator)).ok_or("native session provider bond absent"))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((point, Some((row, operators))))
    }

    fn palw_monetary_material_release_finalized_v1(
        &self,
        root: kaspa_hashes::Hash64,
        consent: kaspa_hashes::Hash64,
        tail: kaspa_hashes::Hash64,
        release_block: BlockHash,
    ) -> Result<bool, String> {
        // This coherent read also refuses incomplete imports, stale tips and
        // missing states. An unsigned absence can never authorize file GC.
        let before = self.lkg_virtual_state.load();
        let (import_before, stable_before) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable_before || import_before.active.is_some() {
            return Err("material GC import is incomplete".into());
        }
        let rows = self.palw_monetary_material_covenants_v1()?;
        if rows.iter().any(|row| row.terms.root == root) {
            return Ok(false);
        }
        if !self.virtual_processor.palw_monetary_material_release_delta_v1(root, consent, tail, release_block)? {
            return Ok(false);
        }
        let finality = self.virtual_processor.virtual_finality_point(&before.ghostdag_data, self.pruning_point());
        for block in [tail, release_block] {
            if !self.services.reachability_service.has_reachability_data(block)
                || !self.services.reachability_service.is_chain_ancestor_of(block, before.ghostdag_data.selected_parent)
                || !self.services.reachability_service.is_chain_ancestor_of(block, finality)
            {
                return Ok(false);
            }
        }
        let after = self.lkg_virtual_state.load();
        let (import, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || import.active.is_some()
            || import.epoch != import_before.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
        {
            return Err("material GC witness changed snapshot/import generation".into());
        }
        Ok(true)
    }

    fn palw_material_terminal_receipt_finalized_v2(
        &self,
        root: kaspa_hashes::Hash64,
        stored_consent: kaspa_hashes::Hash64,
    ) -> Result<Option<(BlockHash, kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialTerminalReceiptV2)>, String>
    {
        use kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryRowV2;
        let Some((pin, state)) = self.palw_finalized_material_recovery_state_v2()? else { return Ok(None) };
        Ok(match state.material_recovery_row_v2(root) {
            Some(PalwMaterialRecoveryRowV2::Terminal(receipt)) if receipt.covers(root, stored_consent) => Some((pin, receipt)),
            _ => None,
        })
    }
    fn palw_material_provider_terminal_receipts_finalized_v4(
        &self,
        provider: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<
            Vec<kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialTerminalReceiptV2>,
        >,
        String,
    > {
        let Some((_, state)) = self.palw_finalized_material_recovery_state_v2()? else {
            return Err("native finalized recovery state unavailable".into());
        };
        kaspa_consensus_core::palw_material_release_recovery_v2::material_provider_terminals_owned_v4(&state, provider)
    }
    fn palw_material_recovery_finalized_daa_v4(&self) -> Result<Option<u64>, String> {
        let Some((pin, _)) = self.palw_finalized_material_recovery_state_v2()? else { return Ok(None) };
        // The header is the native finality witness, never a wall-clock estimate.
        self.headers_store.get_daa_score(pin).map(Some).map_err(|e| e.to_string())
    }
    fn palw_kernel_producer_candidate_v4(
        &self,
        checkpoint: BlockHash,
        producer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        commit: &misaka_palw_kernel::route::SaltedCommitV1,
        salt: &[u8; 64],
        commit_daa: u64,
        expires_daa: u64,
    ) -> Result<kaspa_consensus_core::palw_kernel_producer_candidate_v4::PalwKernelProducerCandidateOwnedV4, String> {
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || generation.active.is_some() {
            return Err("kernel ingress import incomplete".into());
        }
        if !self.services.reachability_service.has_reachability_data(checkpoint)
            || !self.services.reachability_service.is_chain_ancestor_of(checkpoint, before.ghostdag_data.selected_parent)
        {
            return Err("kernel ingress checkpoint is not on own accepted chain".into());
        }
        let daa = self.headers_store.get_daa_score(checkpoint).map_err(|e| e.to_string())?;
        if daa > now || now - daa > kaspa_consensus_core::palw_prospective_kernel_material_v4::PALW_KERNEL_INGRESS_CHECKPOINT_WINDOW_V4
        {
            return Err("kernel ingress checkpoint outside bounded native window".into());
        }
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(checkpoint)
            .map_err(|e| format!("kernel ingress native state unavailable: {e:?}"))?;
        let prepared = self.virtual_processor.palw_kernel_producer_candidate_v4_impl(
            &state,
            checkpoint,
            now,
            producer,
            commit,
            salt,
            commit_daa,
            expires_daa,
        )?;
        let after = self.lkg_virtual_state.load();
        let (generation_after, stable_after) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable_after
            || generation_after.active.is_some()
            || generation.epoch != generation_after.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err("kernel ingress changed native tip/clock/import generation".into());
        }
        Ok(prepared)
    }
    fn palw_kernel_material_consent_accepted_v4(
        &self,
        root: kaspa_hashes::Hash64,
        consent: kaspa_hashes::Hash64,
    ) -> Result<bool, String> {
        use kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryRowV2;
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let (epoch, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || epoch.active.is_some() {
            return Err("kernel consent import incomplete".into());
        }
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(before.ghostdag_data.selected_parent)
            .map_err(|e| format!("kernel consent state unavailable: {e:?}"))?;
        let Some(row) = state.monetary_material_row_v1(root) else { return Ok(false) };
        if row.consent != consent
            || row.terms.native_session.is_some()
            || row.terms.material_manifest.as_ref().is_none_or(|m| m.root != root)
            || now > row.terms.enrollment_until_daa
        {
            return Ok(false);
        }
        let Some(PalwMaterialRecoveryRowV2::Active(recovery)) = state.material_recovery_row_v2(root) else { return Ok(false) };
        if !recovery.generations.iter().any(|g| g.consent == consent) {
            return Ok(false);
        }
        let valid = true;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || end.epoch != epoch.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err("kernel consent native point changed".into());
        }
        Ok(valid)
    }
    fn palw_kernel_material_admission_accepted_v4(
        &self,
        request: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4,
        draft: &kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2,
    ) -> Result<bool, String> {
        use kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryRowV2;
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let (epoch, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || epoch.active.is_some() {
            return Err("kernel admission import incomplete".into());
        }
        let current = self
            .virtual_processor
            .palw_candidate_state_v2_checked(before.ghostdag_data.selected_parent)
            .map_err(|e| format!("kernel material current state unavailable: {e:?}"))?;
        let kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2::FundedEnroll {
            monetary,
            terms,
            ..
        } = draft
        else {
            return Err("kernel admission requires atomic funded enrollment".into());
        };
        if monetary.terms.root != request.intent.manifest.root
            || monetary.terms.material_manifest.as_ref() != Some(&request.intent.manifest)
            || monetary.terms.sponsor != request.intent.producer
            || terms.initial_consent != monetary.id()
        {
            return Err("kernel admission draft/request mismatch".into());
        }
        if now > monetary.terms.enrollment_until_daa {
            return Err("kernel funded enrollment expired before claim admission".into());
        }
        let rid = kaspa_hashes::Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/RELEASE-RECOVERY-CONSENT/V2", terms));
        let matches = |state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2| {
            state
                .monetary_material_row_v1(monetary.terms.root)
                .is_some_and(|row| row.consent == monetary.id() && row.terms == monetary.terms)
                && matches!(state.material_recovery_row_v2(monetary.terms.root),Some(PalwMaterialRecoveryRowV2::Active(ref c)) if c.id()==rid&&c.generations.iter().any(|g|g.consent==monetary.id()))
        };
        if !matches(&current) {
            return Ok(false);
        }
        let result = matches(&current);
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || end.epoch != epoch.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err("kernel admission native point changed".into());
        }
        Ok(result)
    }
    fn palw_kernel_material_ingress_v4(
        &self,
        checkpoint: BlockHash,
        request: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4,
    ) -> Result<kaspa_consensus_core::palw_kernel_material_enrollment_v4::PalwKernelMaterialIngressOwnedV4, String> {
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || generation.active.is_some() {
            return Err("kernel ingress import incomplete".into());
        }
        if !self.services.reachability_service.has_reachability_data(checkpoint)
            || !self.services.reachability_service.is_chain_ancestor_of(checkpoint, before.ghostdag_data.selected_parent)
        {
            return Err("kernel ingress checkpoint is not on own accepted chain".into());
        }
        let daa = self.headers_store.get_daa_score(checkpoint).map_err(|e| e.to_string())?;
        if daa > now || now - daa > kaspa_consensus_core::palw_prospective_kernel_material_v4::PALW_KERNEL_INGRESS_CHECKPOINT_WINDOW_V4
        {
            return Err("kernel ingress checkpoint outside bounded native window".into());
        }
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(checkpoint)
            .map_err(|e| format!("kernel ingress native state unavailable: {e:?}"))?;
        let prepared = self.virtual_processor.palw_kernel_material_ingress_v4_impl(&state, checkpoint, now, request)?;
        let after = self.lkg_virtual_state.load();
        let (generation_after, stable_after) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable_after
            || generation_after.active.is_some()
            || generation.epoch != generation_after.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err("kernel ingress changed native tip/clock/import generation".into());
        }
        Ok(prepared)
    }
    fn palw_kernel_material_context_authority_v4(
        &self,
        context: &kaspa_consensus_core::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4,
        capsule: &kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2,
        provider: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        pubkey: &[u8],
    ) -> Result<(), String> {
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || generation.active.is_some() {
            return Err("kernel context import incomplete".into());
        }
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(before.ghostdag_data.selected_parent)
            .map_err(|e| format!("kernel current native state unavailable: {e:?}"))?;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("kernel context needs ConsensusV2".into());
        };
        state.assert_monetary_material_consistency_v1(&bundle.state).map_err(|e| e.to_string())?;
        state.assert_material_release_recovery_consistency_v2(&bundle.state).map_err(|e| e.to_string())?;
        kaspa_consensus_core::palw_kernel_material_enrollment_v4::require_kernel_context_current_v4(
            &state,
            &bundle.state,
            now,
            context,
            capsule,
            &provider,
            pubkey,
            kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3,
        )?;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || generation.epoch != end.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err("kernel context native point/import changed".into());
        }
        Ok(())
    }
    fn palw_material_generation_authority_v4(
        &self,
        root: kaspa_hashes::Hash64,
        consent: kaspa_hashes::Hash64,
        provider: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        scope: &kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialScopeV1,
    ) -> Result<(), String> {
        self.palw_material_generation_authority_checked_v4(root, consent, provider, scope).map_err(|why| why.to_string())
    }
    fn palw_material_generation_authority_checked_v4(
        &self,
        root: kaspa_hashes::Hash64,
        consent: kaspa_hashes::Hash64,
        provider: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        scope: &kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialScopeV1,
    ) -> Result<(), kaspa_consensus_core::palw_kernel_material_enrollment_v4::PalwMaterialGenerationAuthorityErrorV4> {
        use crate::pipeline::virtual_processor::PalwWeighFaultV2;
        use kaspa_consensus_core::palw_kernel_material_enrollment_v4::PalwMaterialGenerationAuthorityErrorV4 as E;
        use kaspa_consensus_core::palw_state_v2::PalwStateV2Error as S;
        let state_fault = |why: S| match why {
            S::NativeResource(why) => E::Resource(why),
            S::NativeContextUnavailable(kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::Capacity(why)) => E::Resource(why),
            S::NativeContextUnavailable(kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::CapacityUnavailable) => E::CapacityUnavailable,
            why => E::Unavailable(why.to_string()),
        };
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|why| E::Unavailable(why.to_string()))?;
        if !stable || generation.active.is_some() {
            return Err(E::Unavailable("material generation import incomplete".into()));
        }
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(before.ghostdag_data.selected_parent)
            .map_err(|why| match why {
                PalwWeighFaultV2::NativeCapacityUnavailable => E::CapacityUnavailable,
                why => E::Unavailable(format!("material generation native state unavailable: {why:?}")),
            })?;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err(E::Unavailable("material generation needs ConsensusV2".into()));
        };
        state.assert_monetary_material_consistency_v1(&bundle.state).map_err(state_fault)?;
        state.assert_material_release_recovery_consistency_v2(&bundle.state).map_err(state_fault)?;
        kaspa_consensus_core::palw_kernel_material_enrollment_v4::require_material_generation_current_checked_v4(
            &state,
            &bundle.state,
            now,
            root,
            consent,
            provider,
            scope,
        )?;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|why| E::Unavailable(why.to_string()))?;
        if !stable
            || end.active.is_some()
            || end.epoch != generation.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err(E::Unavailable("material generation native point/import changed".into()));
        }
        Ok(())
    }
    fn palw_kernel_material_replacement_v4(
        &self,
        context: &kaspa_consensus_core::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4,
        bundle: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
        intent: &kaspa_consensus_core::palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementIntentV4,
    ) -> Result<kaspa_consensus_core::palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementOwnedV4, String> {
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || generation.active.is_some() {
            return Err("replacement native import incomplete".into());
        }
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(before.ghostdag_data.selected_parent)
            .map_err(|e| format!("replacement native state unavailable: {e:?}"))?;
        let quote = self.virtual_processor.palw_kernel_material_replacement_v4_impl(&state, now, context, bundle, intent)?;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || end.epoch != generation.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err("replacement native point/clock/import changed".into());
        }
        Ok(quote)
    }
    fn palw_kernel_material_capsule_unused_finalized_v4(
        &self,
        capsule: &kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2,
    ) -> Result<bool, String> {
        use kaspa_consensus_core::{
            palw_kernel_material_enrollment_v4::kernel_material_capsule_identity_v4,
            palw_kernel_material_replacement_v4::material_generation_present_v4,
        };
        let (enrollment, consent, until) = kernel_material_capsule_identity_v4(capsule)?;
        let root = enrollment.terms.root;
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || generation.active.is_some() {
            return Err("generation expiry native import incomplete".into());
        }
        let current = self
            .virtual_processor
            .palw_candidate_state_v2_checked(before.ghostdag_data.selected_parent)
            .map_err(|e| format!("expiry current state unavailable: {e:?}"))?;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("generation expiry needs ConsensusV2".into());
        };
        if bundle.state.material_release_recovery_at_v2(now).is_none() {
            return Ok(false);
        }
        current.assert_monetary_material_consistency_v1(&bundle.state).map_err(|e| e.to_string())?;
        current.assert_material_release_recovery_consistency_v2(&bundle.state).map_err(|e| e.to_string())?;
        if material_generation_present_v4(&current, root, consent)? {
            return Ok(false);
        }
        let Some((pin, finalized)) = self.palw_finalized_material_recovery_state_v2()? else { return Ok(false) };
        let parent = self.ghostdag_store.get_data(pin).map_err(|e| e.to_string())?.selected_parent;
        // A positive finalized parent clock, not local elapsed time. No old
        // unbounded Replace may be represented by this bounded capsule helper.
        if self.headers_store.get_daa_score(parent).map_err(|e| e.to_string())? <= until {
            return Ok(false);
        }
        finalized.assert_monetary_material_consistency_v1(&bundle.state).map_err(|e| e.to_string())?;
        if material_generation_present_v4(&finalized, root, consent)? {
            return Ok(false);
        }
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || end.epoch != generation.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err("generation expiry native point/clock/import changed".into());
        }
        Ok(true)
    }
    fn palw_kernel_material_query_authority_v4(
        &self,
        query: &kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialUnitQueryV4,
        requester: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        pubkey: &[u8],
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<
            kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialQueryAuthorityV4,
        >,
        String,
    > {
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || generation.active.is_some() {
            return Err("material query import incomplete".into());
        }
        let point = before.ghostdag_data.selected_parent;
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(point)
            .map_err(|e| format!("material query native state unavailable: {e:?}"))?;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("material query needs ConsensusV2".into());
        };
        state.assert_monetary_material_consistency_v1(&bundle.state).map_err(|e| e.to_string())?;
        state.assert_material_release_recovery_consistency_v2(&bundle.state).map_err(|e| e.to_string())?;
        let authority = kaspa_consensus_core::palw_kernel_material_query_v4::kernel_material_query_authority_v4(
            &state,
            &bundle.state,
            point,
            now,
            query,
            requester,
            pubkey,
        )?;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || end.epoch != generation.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err("material query native point/clock/import changed".into());
        }
        Ok(authority)
    }
    fn palw_exec_origin_coverage_v1(
        &self,
        listed: usize,
    ) -> Result<kaspa_consensus_core::palw_work_slice_v2::PalwExecOriginCoverageOwnedV1, String> {
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || generation.active.is_some() {
            return Err("historical header census import incomplete".into());
        }
        let before = self.lkg_virtual_state.load();
        let now = self.get_virtual_daa_score();
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(before.ghostdag_data.selected_parent)
            .map_err(|e| format!("historical header census state unavailable: {e:?}"))?;
        let census = kaspa_consensus_core::palw_exec_origin_coverage_v1::palw_exec_origin_coverage_checked_v2(&state, listed.min(4096), |block| {
            self.headers_store.historical_header_borsh_len_v2(block).optional()
        }).map_err(|e| e.to_string())?;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || end.epoch != generation.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
            || now != self.get_virtual_daa_score()
        {
            return Err("historical header census point/clock/import changed".into());
        }
        Ok(census)
    }
    fn palw_native_recovery_session_quote_v4(
        &self,
        checkpoint: Option<BlockHash>,
        daa: u64,
        class: kaspa_hashes::Hash64,
        executor: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        providers: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<
            kaspa_consensus_core::palw_material_session_enrollment_v4::PalwNativeRecoverySessionQuoteV4,
        >,
        String,
    > {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("recovery enrollment quote needs ConsensusV2".into());
        };
        let before = self.lkg_virtual_state.load();
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || generation.active.is_some() {
            return Err("recovery quote import incomplete".into());
        }
        let point = checkpoint.unwrap_or(before.ghostdag_data.selected_parent);
        if !self.services.reachability_service.has_reachability_data(point)
            || !self.services.reachability_service.is_chain_ancestor_of(point, before.ghostdag_data.selected_parent)
        {
            return Err("recovery checkpoint not on current accepted chain".into());
        }
        let point_daa = self.headers_store.get_daa_score(point).map_err(|e| e.to_string())?;
        if daa < point_daa || point_daa.saturating_add(600) < daa {
            return Err("recovery quote checkpoint/DAA exceeds bounded window".into());
        }
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(point)
            .map_err(|e| format!("recovery quote state unavailable: {e:?}"))?;
        state.assert_monetary_material_consistency_v1(&bundle.state).map_err(|e| e.to_string())?;
        state.assert_material_release_recovery_consistency_v2(&bundle.state).map_err(|e| e.to_string())?;
        let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.config.params.net.to_string().as_bytes(),
            Some(self.config.params.genesis.hash),
        );
        let quote = kaspa_consensus_core::palw_material_session_enrollment_v4::native_recovery_session_quote_v4(
            &state,
            &bundle.state,
            point,
            daa,
            network,
            class,
            executor,
            &providers,
        )?;
        let after = self.lkg_virtual_state.load();
        let (generation_after, stable_after) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable_after
            || generation_after.active.is_some()
            || generation.epoch != generation_after.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
        {
            return Err("recovery quote changed tip/import generation".into());
        }
        Ok(quote)
    }
    fn palw_material_recovery_enrollment_finalized_v4(
        &self,
        root: kaspa_hashes::Hash64,
        consent: kaspa_hashes::Hash64,
        recovery: kaspa_hashes::Hash64,
    ) -> Result<bool, String> {
        let Some((_, state)) = self.palw_finalized_material_recovery_state_v2()? else { return Ok(false) };
        Ok(state
            .material_recovery_row_v2(root)
            .is_some_and(|row| row.contract().id() == recovery && row.contract().generations.iter().any(|g| g.consent == consent)))
    }
    fn palw_material_provider_ack_checkpoint_finalized_v2(
        &self,
        provider: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<
        Option<(BlockHash, kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialProviderAckCheckpointV2)>,
        String,
    > {
        let Some((pin, state)) = self.palw_finalized_material_recovery_state_v2()? else { return Ok(None) };
        Ok(state.material_provider_ack_checkpoint_v2(&provider).map(|checkpoint| (pin, checkpoint)))
    }

    fn palw_material_challenge_finalized_v4(
        &self,
        actor: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<kaspa_consensus_core::palw_material_challenge_history_v4::PalwMaterialChallengeFinalizedViewV4>, String> {
        use kaspa_consensus_core::{
            palw_material_challenge_checkpoint_v4::material_challenge_checkpoint_from_route_v4,
            palw_material_challenge_history_v4::{
                PalwMaterialChallengeFinalizedViewV4, material_challenge_history_base_anchor_v4,
                material_challenge_history_from_route_v4,
            },
        };
        // The helper pins import generation, selected chain, finalized header,
        // its committed parent state root, and recovery consistency together.
        let Some((pin, state)) = self.palw_finalized_material_recovery_state_v2()? else { return Ok(None) };
        let header = self.headers_store.get_header(pin).map_err(|e| e.to_string())?;
        let state_root =
            kaspa_consensus_core::palw_state_proof_v1::state_root_of_pinned_header_v1(&header, pin).map_err(|e| e.to_string())?;
        let state_point = self.ghostdag_store.get_data(pin).map_err(|e| e.to_string())?.selected_parent;
        let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.config.params.net.to_string().as_bytes(),
            Some(self.config.params.genesis.hash),
        );
        let anchor = if let Some(route) = state.kernel_route() {
            if kaspa_hashes::Hash64::from_bytes(route.header.policy.network_domain) != network {
                return Err("finalized challenge route differs from configured network incarnation".into());
            }
            let checkpoint = material_challenge_checkpoint_from_route_v4(route, actor).map_err(|e| e.to_string())?;
            let history = material_challenge_history_from_route_v4(route, actor).map_err(|e| e.to_string())?;
            material_challenge_history_base_anchor_v4(network, actor, checkpoint.as_ref(), history.as_ref()).map_err(str::to_string)?
        } else {
            material_challenge_history_base_anchor_v4(network, actor, None, None).map_err(str::to_string)?
        };
        // All fields are fixed and owned. This is the own API's observation;
        // receiving/decoding its bytes remotely does not establish authority.
        let view =
            PalwMaterialChallengeFinalizedViewV4 { version: 4, finality_block: pin, state_point, state_root, network, actor, anchor };
        view.validate().map_err(str::to_string)?;
        Ok(Some(view))
    }

    fn palw_monetary_material_unused_offer_finalized_v1(
        &self,
        root: kaspa_hashes::Hash64,
        enrollment_until: u64,
    ) -> Result<bool, String> {
        let before = self.lkg_virtual_state.load();
        let (import_before, stable_before) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable_before || import_before.active.is_some() {
            return Err("unused-offer import is incomplete".into());
        }
        if self.palw_monetary_material_covenants_v1()?.iter().any(|row| row.terms.root == root) {
            return Ok(false);
        }
        let current_state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(before.ghostdag_data.selected_parent)
            .map_err(|e| format!("unused-offer current state unavailable: {e:?}"))?;
        let recovery_present = |state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2| {
            state.kernel_route().is_some_and(|route| {
                route.aux.keys().any(|(table, key)| {
                    *table == kaspa_consensus_core::palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2
                        && key.as_slice() == root.as_byte_slice()
                })
            })
        };
        // Terminal generations need their ordinary durable GC completion and
        // ACK path. An expiry shortcut must never erase that receipt evidence.
        if recovery_present(&current_state) {
            return Ok(false);
        }
        let finality = self.virtual_processor.virtual_finality_point(&before.ghostdag_data, self.pruning_point());
        if !self.services.reachability_service.has_reachability_data(finality)
            || !self.services.reachability_service.is_chain_ancestor_of(finality, before.ghostdag_data.selected_parent)
            || self.headers_store.get_daa_score(finality).map_err(|e| e.to_string())? <= enrollment_until
        {
            return Ok(false);
        }
        // Current absence could be caused by an unfinalized release, which
        // an admitted reorg may undo. Require absence AT the finalized block.
        let finalized_state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(finality)
            .map_err(|e| format!("unused-offer finalized state unavailable: {e:?}"))?;
        if finalized_state.monetary_material_row_v1(root).is_some() {
            return Ok(false);
        }
        if recovery_present(&finalized_state) {
            return Ok(false);
        }
        let after = self.lkg_virtual_state.load();
        let (import_after, stable_after) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable_after
            || import_after.active.is_some()
            || import_before.epoch != import_after.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
        {
            return Err("unused offer changed snapshot/import generation".into());
        }
        Ok(true)
    }

    fn palw_monetary_aux_page_v1(
        &self,
        block: BlockHash,
        row: u64,
        offset: u64,
        limit: usize,
    ) -> Result<kaspa_consensus_core::palw_monetary_material_rpc_v1::PalwMonetaryAuxPageV1, String> {
        self.virtual_processor.palw_monetary_aux_page_v1_impl(block, row, offset, limit)
    }

    fn palw_state_carriage_page_v1(
        &self,
        block: BlockHash,
        offset: u64,
        limit: usize,
    ) -> Result<kaspa_consensus_core::palw_state_carriage_rpc_v1::PalwStateCarriagePageV1, String> {
        self.virtual_processor.palw_state_carriage_page_v1_impl(block, offset, limit)
    }

    /// ADR-0080 design A, on the node: the materialized tip, then the row itself. The tip read is
    /// the same snapshot every other `palw_*` answer here takes.
    fn palw_model_market_v1(
        &self,
        line_id: kaspa_hashes::Hash64,
    ) -> Option<(
        kaspa_consensus_core::palw_model_market_v1::PalwModelMarketV1,
        bool,
        kaspa_consensus_core::palw_state_v2::PalwClassStatusV2,
    )> {
        let state = self.palw_state_v2_tip()?;
        // ADR-0088 Decision 9: the market is keyed by line; a founding line IS its class until a
        // row is written, so a class id resolves through the same reader the fold uses.
        let line = state.model_line_or_founding(&line_id)?;
        let class = state.class(&line.class_id)?;
        let daa = state.last_point().map(|p| p.daa_score).unwrap_or(0);
        let closed_line = !line.is_active();
        // **ADR-0162 Decision 1: past the virtual fence every line's market is open from the line's
        // creation**, and this answers what the fold of the virtual's next block moves — the row, or
        // the opening every surface synthesizes (`X = V`, the supply in the curve), the same function
        // the fold calls. It answers as a market (`true`): there is no "not opened" line past the
        // fence. The row's `closed_to_buys` is a record of its last move there, not the gate, so the
        // answer carries the line's own state and the service adds the class's and the registry's.
        if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
            && let Some(fence_daa) = self.config.params.palw_model_virtual_v1_from_at(self.get_virtual_daa_score())
            && let Some(mut market) = state.model_market_in_force_v2(&line_id, fence_daa, &bundle.state.base_class_id())
        {
            market.closed_to_buys = closed_line;
            return Some((market, true, class.status.clone()));
        }
        match state.model_market(&line_id) {
            Some(market) => {
                let mut market = *market;
                market.closed_to_buys |= closed_line;
                Some((market, true, class.status.clone()))
            }
            None => {
                let mut market = kaspa_consensus_core::palw_model_market_v1::PalwModelMarketV1::seed_v1(
                    daa,
                    0,
                    kaspa_consensus_core::Hash64::default(),
                );
                market.closed_to_buys = closed_line;
                Some((market, false, class.status.clone()))
            }
        }
    }

    /// The 2026-09-23 Position route matrix, P-B3: the processor holds the fences, so it resolves the
    /// extras the gate is asked under — the same way the producer's class gate is (route-matrix #7).
    fn palw_model_market_gate_v1(
        &self,
        line_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::api::PalwModelMarketGateReadV1> {
        self.virtual_processor.palw_model_market_gate_v1_impl(line_id)
    }

    fn palw_model_positions_v1(&self, holder: kaspa_hashes::Hash64) -> Vec<(kaspa_hashes::Hash64, u64)> {
        let Some(state) = self.palw_state_v2_tip() else {
            return Vec::new();
        };
        state.model_positions_of(&holder)
    }

    /// ADR-0088 Decision 12: the tip as every registry reader takes it — the same cached snapshot
    /// the market reads, `None` off ConsensusV2 or before the first state was written.
    fn palw_model_line_v1(&self, line_id: kaspa_hashes::Hash64) -> Option<kaspa_consensus_core::api::PalwModelLineReadV1> {
        let state = self.palw_state_v2_tip()?;
        let row = Self::palw_model_line_row_read_v1(&state, line_id)?;
        let tip_daa = state.last_point().map(|p| p.daa_score).unwrap_or(0);
        let current_root = state.model_version(&line_id, row.line.current).map(|v| v.root);
        let roots_in_force = state.class_roots_in_force(&row.line.class_id, tip_daa);
        // ADR-0095: resolved here, at the tip, so every caller sees the same answer §4.6 gives.
        let benefits = state.model_benefits(&line_id).map(|b| kaspa_consensus_core::api::PalwModelBenefitsReadV1 {
            row: b.clone(),
            in_effect: state.model_benefit_tiers_in_effect(&line_id, tip_daa),
            lapse: state.model_benefit_lapse(&line_id, tip_daa),
            enforced_lead_daa: state.model_benefit_enforced_lead(&line_id, tip_daa),
        });
        // ADR-0101 / ADR-0143: the facts a client checks a provider's descriptor against, read at
        // the same tip as everything above so one answer describes one height.
        let service_facts = state.line_service_facts_v1(
            &row.line.class_id,
            &line_id,
            tip_daa,
            self.config.params.palw_artifact_root_ownership_at(tip_daa),
        );
        Some(kaspa_consensus_core::api::PalwModelLineReadV1 { row, current_root, roots_in_force, tip_daa, benefits, service_facts })
    }

    /// The holder's rows with ADR-0095's tenure and tier, all from the one cached tip snapshot (the
    /// 2026-09-23 Position route matrix, P-B2). A read: nothing here reaches a rule or the root.
    fn palw_model_positions_read_v1(&self, holder: kaspa_hashes::Hash64) -> kaspa_consensus_core::api::PalwModelPositionsReadV1 {
        let Some(state) = self.palw_state_v2_tip() else {
            return Default::default();
        };
        kaspa_consensus_core::api::PalwModelPositionsReadV1::at_tip(&state, &holder)
    }

    fn palw_model_version_v1(
        &self,
        line_id: kaspa_hashes::Hash64,
        version: u32,
    ) -> Option<kaspa_consensus_core::api::PalwModelVersionReadV1> {
        let state = self.palw_state_v2_tip()?;
        let row = state.model_version(&line_id, version)?;
        let evaluations = state.model_evaluations_of(&line_id, version).into_iter().map(|(b, e)| (*b, e.clone())).collect();
        let tip_daa = state.last_point().map(|p| p.daa_score).unwrap_or(0);
        Some(kaspa_consensus_core::api::PalwModelVersionReadV1 { version: row, evaluations, tip_daa })
    }

    fn palw_evm_view_v1(
        &self,
    ) -> Option<(
        Arc<kaspa_consensus_core::evm::model_market::PalwEvmViewV1>,
        kaspa_consensus_core::evm::model_market::PalwEvmMarketFencesV1,
    )> {
        let state_params = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => &bundle.state,
            _ => return None,
        };
        let state = self.palw_state_v2_tip()?;
        // The fences the executor resolves for the block built on this tip use THAT block's
        // DAA score; the tip's own score is the closest the simulator can read without a
        // template. They differ only in the one block that crosses an activation.
        let tip_daa = state.last_point().map(|p| p.daa_score).unwrap_or(0);
        let fences = self.config.params.palw_evm_market_fences_at(tip_daa);
        // **ADR-0089 Decision 9: below the fence there is no window** (mainnet audit 2026-09-06,
        // M-13). This resolved the fences and then returned them ALONGSIDE the view instead of
        // consulting them, so the whole flattening — every class, every line, every version, and a
        // `BTreeMap` copy of the markets and of every position row — was built for every `eth_call`
        // and `eth_estimateGas` on any ConsensusV2 network, including a carded mainnet, with no
        // per-caller budget. It is cheap today only because the tables are empty, which is not a
        // bound, it is a coincidence.
        //
        // `None` is the documented "the market's doors are not registered, whatever the fences say"
        // (`kaspa_evm::sim::EthCallEnv::palw_view`), which is exactly Decision 9's sentence: below
        // the fence the four addresses and the facades are empty accounts. The consumer in
        // `kaspad/src/eth_rpc.rs` already maps `None` to an empty view and default fences, and its
        // second call site deliberately passes `None` for the same reason.
        //
        // The registry reads (F010–F012) are ADR-0088's fence and the market reads ADR-0087's, so
        // the window is built when ANY of the three is in force and not otherwise.
        if !fences.any_active() {
            return None;
        }
        // P-B3 (the 2026-09-23 Position route matrix): the same builder consensus uses, so an
        // `eth_call` / `eth_estimateGas` of a seed or a buy the fold's market gate refuses reverts
        // `ClassNotEligible()` here as it will in the block. The tip is the next block's selected
        // parent, and its score stands in for that block's, as the fences above do.
        let tip = state.last_point().map(|p| p.block).unwrap_or_default();
        Some((Arc::new(self.virtual_processor.palw_evm_view_with_market_gate_v1(&state, state_params, tip, tip_daa)), fences))
    }

    fn palw_evm_view_at_owned_v2(
        &self,
        target: BlockHash,
    ) -> Result<Option<kaspa_consensus_core::evm::view_owned_v2::NativeEvmViewAtV2>, kaspa_consensus_core::evm::view_owned_v2::NativeEvmViewFaultV2> {
        use kaspa_consensus_core::evm::view_owned_v2::NativeEvmViewFaultV2 as E;
        let params = &self.config.params;
        let bundle = match &params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle,
            _ => return Ok(None),
        };
        // Keep the exact header, import epoch and projection in one reader-first
        // pruning session. The internal state reader supports recursive reads.
        let _session = self.pruning_lock.blocking_read();
        // A reader session prevents pruning, but begin_import may still change
        // epoch. Preserve its typed result before header read and validate only
        // for an active view: dormant windows do no state work and return None.
        let generation = self.virtual_processor.evm_asof_import_status_v2();
        let header = self.headers_store.get_header(target).map_err(|e| match e {
            kaspa_database::prelude::StoreError::KeyNotFound(_) => E::HistoryUnavailable,
            kaspa_database::prelude::StoreError::LocalResourceUnavailable(reason) => E::Resource(
                kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<kaspa_consensus_core::header::Header>("historical EVM header", &reason)),
            other => E::Store(other.to_string()),
        })?;
        let fences = params.palw_evm_market_fences_at(header.daa_score);
        if !fences.any_active() { return Ok(None); }
        let (generation, stable) = generation.map_err(crate::pipeline::virtual_processor::NativeEvmAsOfFaultV2::into_public_v2)?;
        if !stable || generation.active.is_some() { return Err(E::Context("import incomplete".into())); }
        let state = self.virtual_processor.palw_evm_post_state_owned_v2(target)
            .map_err(crate::pipeline::virtual_processor::NativeEvmAsOfFaultV2::into_public_v2)?
            .ok_or(E::HistoryUnavailable)?;
        if state.last_point().is_some_and(|p| p.block != target || p.daa_score != header.daa_score) {
            return Err(E::Context("post-target state/header point differs".into()));
        }
        let gate = kaspa_consensus_core::palw_state_v2::NativeEvmMarketGateInputsV2::at_params_v2(params, header.daa_score);
        let view = state.evm_view_owned_v2(kaspa_consensus_core::evm::EVM_CHAIN_ID, bundle.state.base_class_id(), gate)
            .map_err(E::Resource)?;
        #[cfg(test)]
        evm_view_projection_hook_v2::invoke(self);
        let (after, stable) = self.virtual_processor.evm_asof_import_status_v2()
            .map_err(crate::pipeline::virtual_processor::NativeEvmAsOfFaultV2::into_public_v2)?;
        if !stable || after.active.is_some() || after.epoch != generation.epoch {
            return Err(E::Context("import generation changed during historical view projection".into()));
        }
        Ok(Some((view, fences)))
    }

    fn palw_evm_replay_market_owned_v2(
        &self,
        accepting: BlockHash,
    ) -> Result<Option<kaspa_consensus_core::evm::view_owned_v2::NativeEvmReplayMarketV2>, kaspa_consensus_core::evm::view_owned_v2::NativeEvmViewFaultV2> {
        self.read_evm_replay_market_owned_v2(accepting)
    }

    fn palw_model_lines_v1(&self, class_id: kaspa_hashes::Hash64) -> Option<Vec<kaspa_consensus_core::api::PalwModelLineRowReadV1>> {
        let state = self.palw_state_v2_tip()?;
        state.class(&class_id)?;
        // The founding line first (its id is the class id, written or synthesised), then every
        // other row of the class in id order — the order `class_line_rows` walks them in.
        let mut ids: Vec<kaspa_hashes::Hash64> = vec![class_id];
        ids.extend(state.class_line_rows(&class_id).into_iter().map(|(id, _)| *id).filter(|id| *id != class_id));
        Some(ids.into_iter().filter_map(|id| Self::palw_model_line_row_read_v1(&state, id)).collect())
    }

    fn palw_model_proposals_v1(
        &self,
        line_id: kaspa_hashes::Hash64,
    ) -> Option<Vec<(kaspa_hashes::Hash64, kaspa_consensus_core::palw_model_lines_v1::PalwModelProposalV1)>> {
        let state = self.palw_state_v2_tip()?;
        state.model_line_or_founding(&line_id)?;
        Some(state.model_proposals_of(&line_id).into_iter().map(|(id, p)| (*id, p.clone())).collect())
    }

    fn palw_court_close_group_v1(
        &self,
        session_id: kaspa_hashes::Hash64,
        side: kaspa_consensus_core::palw_state_v2::PalwCourtSideV1,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwCourtCloseGroupV2> {
        let state_params = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => &bundle.state,
            _ => return None,
        };
        // Audit M-7: the same materialized tip every other `palw_*` answer takes, decoded once per
        // tip row rather than once per request. W5's chunk bytes are inside that carriage — up to
        // `2 x 27 x 100 KB` per open session — and this call is reachable unauthenticated.
        let (_chain_point, state) = self.virtual_processor.palw_v2_reader_state(state_params).ok().flatten()?;
        state.court_close_group(&session_id, side).cloned()
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
        if self.config.params.palw_dns_retired_at(self.get_sink_daa_score_timestamp().daa_score) {
            return None;
        }
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
        // Cost/exposure note: like the sibling `get_attestation_quality_deficits`, this clones the
        // whole store per call
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
        if self.config.params.palw_dns_retired_at(self.get_sink_daa_score_timestamp().daa_score) {
            return None;
        }
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

    fn get_attestation_quality_deficits(&self) -> Vec<AttestationQualityDeficit> {
        let Some(dns_params) = self.config.params.dns_params.as_ref() else {
            return Vec::new();
        };
        let virtual_daa_score = self.get_virtual_daa_score();
        if virtual_daa_score < dns_params.dns_activation_daa_score || !dns_params.dns_v3_params_consistent() {
            return Vec::new();
        }

        let sink = self.get_sink();
        // RFC-0012: past the retirement no attestation is admitted, so no deficit can be cured — report none (lane R12, 2026-10-11).
        if self.headers_store.get_daa_score(sink).is_ok_and(|daa| self.config.params.palw_dns_retired_at(daa)) {
            return Vec::new();
        }
        let anchors = self.virtual_processor.canonical_anchors_in_window(sink, dns_params, dns_params.stake_score_window_blue_score);
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
            *entry = entry.saturating_add(c.signed_weight as u64);
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
        if self.config.params.palw_dns_retired_at(self.get_sink_daa_score_timestamp().daa_score) {
            return Vec::new();
        }
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
            *entry = entry.saturating_add(c.signed_weight as u64);
            if c.bond_outpoint == bond_outpoint && c.validator_id == bond.validator_pubkey_hash {
                signed_by_this_bond.insert(c.epoch);
            }
        }

        let mut deficient = Vec::new();
        let mut fallback = Vec::new();
        for (epoch, anchor) in
            self.virtual_processor.canonical_anchors_in_window(sink, dns_params, dns_params.stake_score_window_blue_score)
        {
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

    fn get_precommit_duty(
        &self,
        validator_id: kaspa_consensus_core::Hash64,
        bond_outpoint: TransactionOutpoint,
    ) -> Option<kaspa_consensus_core::dns_finality::PrecommitDuty> {
        // ADR-0128 Decision 6: read from the chain at the sink — `round_active` is the BFT gate's
        // fence there, the lock is the one the chain shows, and the due epochs are the StakeScore
        // window's that reached round one and that this bond has not precommitted.
        self.virtual_processor.dns_bft_precommit_duty(self.get_sink(), validator_id, bond_outpoint)
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
            .get_daa_score(self.get_header_download_hint())
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

    fn get_dns_reserve_balance(&self, block: BlockHash) -> Option<u64> {
        let _guard = self.pruning_lock.blocking_read();
        self.dns_reserve_balance_checked_v1(block).ok().flatten()
    }

    fn get_palw_acceptance_reads_v1(
        &self,
        blocks: Vec<BlockHash>,
        outpoints: Vec<TransactionOutpoint>,
    ) -> ConsensusResult<kaspa_consensus_core::api::PalwAcceptanceReadsV1> {
        use crate::model::stores::virtual_state::VirtualStateStoreReader;
        if blocks.len() > 64 || outpoints.len() > 64 {
            return Err(ConsensusError::General("at most 64 reserve blocks and 64 outpoints per acceptance read"));
        }
        let _pruning = self.pruning_lock.blocking_read();
        let virtual_stores = self.virtual_stores.read();
        let unavailable = |e| ConsensusError::GeneralOwned(format!("PALW acceptance read: {e}"));
        let state = virtual_stores.state.get().map_err(unavailable)?;
        let reserve_balances = blocks
            .into_iter()
            .map(|block| self.dns_reserve_balance_checked_v1(block).map(|balance| (block, balance)))
            .collect::<ConsensusResult<Vec<_>>>()?;
        let outpoints = outpoints
            .into_iter()
            .map(|outpoint| {
                UtxoSetStoreReader::get(&virtual_stores.utxo_set, &outpoint)
                    .optional()
                    .map(|entry| (outpoint, entry.map(|entry| entry.as_ref().clone())))
                    .map_err(unavailable)
            })
            .collect::<ConsensusResult<Vec<_>>>()?;
        Ok(kaspa_consensus_core::api::PalwAcceptanceReadsV1 {
            sink: state.ghostdag_data.selected_parent,
            virtual_daa: state.daa_score,
            reserve_balances,
            outpoints,
        })
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

    fn validate_pruning_proof_standalone(&self, proof: &PruningPointProof) -> Result<(), PruningImportError> {
        self.services.pruning_proof_manager.validate_pruning_point_proof_standalone(proof)
    }

    fn validate_pruning_proof(
        &self,
        proof: &PruningPointProof,
        proof_metadata: &PruningProofMetadata,
    ) -> Result<(), PruningImportError> {
        self.services.pruning_proof_manager.validate_pruning_point_proof(proof, proof_metadata)
    }

    fn apply_pruning_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]) -> PruningImportResult<()> {
        // Production proof/history context is established on fresh staging before
        // it receives an import session. Legacy context mutation cannot reset a
        // live or completed session, including one naming the same target.
        self.virtual_processor
            .with_pruning_import_session(None, |_| self.services.pruning_proof_manager.apply_proof(proof, trusted_set))
    }

    fn import_pruning_points(&self, pruning_points: PruningPointsList) -> PruningImportResult<()> {
        self.virtual_processor
            .with_pruning_import_session(None, |_| self.services.pruning_proof_manager.import_pruning_points(&pruning_points))
    }

    fn begin_pruning_point_import(&self, point: BlockHash) -> PruningImportResult<PruningImportSessionV1> {
        self.virtual_processor.begin_pruning_point_import(point)
    }

    fn pruning_import_session(&self) -> PruningImportResult<PruningImportSessionStateV1> {
        self.virtual_processor.pruning_import_session()
    }

    fn pruning_import_status(&self) -> PruningImportResult<(PruningImportSessionStateV1, bool)> {
        self.virtual_processor.pruning_import_status()
    }

    fn append_imported_pruning_point_utxos(
        &self,
        chunk: &[(TransactionOutpoint, UtxoEntry)],
        multiset: &mut MuHash,
    ) -> PruningImportResult<()> {
        self.virtual_processor
            .with_pruning_import_session(None, |_| self.virtual_processor.append_pruning_import_utxos(None, chunk, multiset))
    }

    fn append_imported_pruning_point_utxos_for_session(
        &self,
        session: PruningImportSessionV1,
        chunk: &[(TransactionOutpoint, UtxoEntry)],
        multiset: &mut MuHash,
    ) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(Some(session), |state| {
            if !state.utxos_started || state.utxos_imported {
                return Err(PruningImportError::PruningImportSessionRefused {
                    point: session.point,
                    epoch: session.epoch,
                    reason: "UTXO append is outside the active stream phase".into(),
                });
            }
            self.virtual_processor.append_pruning_import_utxos(Some(session.point), chunk, multiset)
        })
    }

    fn import_pruning_point_utxo_set(&self, point: BlockHash, multiset: MuHash) -> PruningImportResult<()> {
        self.virtual_processor
            .with_pruning_import_session(None, |_| self.virtual_processor.import_pruning_point_utxo_set(point, multiset))
    }

    fn import_pruning_point_utxo_set_for_session(&self, session: PruningImportSessionV1, multiset: MuHash) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(Some(session), |state| {
            if !state.utxos_started || state.utxos_imported {
                return Err(PruningImportError::PruningImportSessionRefused {
                    point: session.point,
                    epoch: session.epoch,
                    reason: "UTXO finalize is outside the active stream phase".into(),
                });
            }
            self.virtual_processor.import_pruning_point_utxo_set(session.point, multiset)?;
            let mut state = state.clone();
            state.utxos_imported = true;
            self.virtual_processor.set_pruning_import_phase(&state)
        })
    }

    fn import_pruning_point_evm_state_for_session(
        &self,
        session: PruningImportSessionV1,
        header: kaspa_consensus_core::evm::EvmExecutionHeader,
        snapshot: kaspa_consensus_core::evm::EvmStateSnapshot,
    ) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(Some(session), |_| {
            self.virtual_processor.import_pruning_point_evm_state(session.point, header, snapshot)
        })
    }

    fn import_pruning_point_palw_state_for_session(
        &self,
        session: PruningImportSessionV1,
        carriage: kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
    ) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(Some(session), |_| {
            self.virtual_processor.import_pruning_point_palw_state(session.point, carriage)
        })
    }

    fn import_pruning_point_palw_snapshot_for_session(
        &self,
        session: PruningImportSessionV1,
        carriage: kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
        declarations: Vec<(kaspa_hashes::Hash64, Vec<u8>)>,
        complete: bool,
    ) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(Some(session), |_| {
            self.virtual_processor.import_pruning_point_palw_snapshot(session.point, carriage, Some((declarations, complete)))
        })
    }

    fn import_pruning_point_overlay_snapshot_for_session(
        &self,
        session: PruningImportSessionV1,
        snapshot: kaspa_consensus_core::dns_finality::OverlaySnapshot,
    ) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(Some(session), |_| {
            self.virtual_processor.import_pruning_point_overlay_snapshot(session.point, snapshot)
        })
    }

    // kaspa-pq ADR-0022: pruned-IBD EVM + overlay snapshot transfer.
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
        self.virtual_processor.with_pruning_import_session(None, |_| {
            self.virtual_processor.import_pruning_point_evm_state(pruning_point, evm_header, snapshot)
        })
    }

    fn pruning_point_overlay_snapshot(&self) -> Option<kaspa_consensus_core::dns_finality::PruningPointOverlaySnapshot> {
        self.virtual_processor.pruning_point_overlay_snapshot()
    }

    fn palw_seat_duties_v2(
        &self,
        mine: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2> {
        self.virtual_processor.palw_seat_duties_v2_impl(&mine)
    }

    fn palw_tir_shard_watch_duties_v1(
        &self,
        watcher: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2> {
        self.virtual_processor.palw_tir_shard_watch_duties_v1_impl(&watcher)
    }

    fn palw_court_duties_v2(
        &self,
        mine: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2> {
        self.virtual_processor.palw_court_duties_v2_impl(&mine)
    }

    fn palw_held_pursuit_seeds_v1(
        &self,
        mine: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> kaspa_consensus_core::palw_producer_v2::PalwHeldPursuitSeedsV1 {
        self.virtual_processor.palw_held_pursuit_seeds_v1_impl(&mine)
    }

    fn palw_da_duties_v2(
        &self,
        mine: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwDaDutyV2> {
        self.virtual_processor.palw_da_duties_v2_impl(&mine)
    }

    fn palw_disclosure_duties_v1(
        &self,
        mine: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> kaspa_consensus_core::palw_producer_v2::PalwDisclosureDutiesV1 {
        self.virtual_processor.palw_disclosure_duties_v1_impl(&mine)
    }

    fn palw_da_accusation_check_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
        accuser: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1> {
        self.virtual_processor.palw_da_accusation_check_v1_impl(claim, accuser)
    }

    fn palw_staged_paid_authority_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedPaidAuthorityV1>, String> {
        self.virtual_processor.palw_staged_paid_authority_v1_impl(key)
    }

    fn palw_staged_owned_context_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<
        Option<(
            kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
            kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        )>,
        String,
    > {
        self.virtual_processor.palw_staged_owned_context_v1_impl(key)
    }

    fn palw_staged_owned_progress_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1>, String> {
        self.virtual_processor.palw_staged_owned_progress_v1_impl(key)
    }

    fn palw_staged_owned_seed_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1>, String> {
        self.virtual_processor.palw_staged_owned_seed_v1_impl(key)
    }

    fn palw_tokenizer_artifact_inventory_v1(
        &self,
        class: kaspa_consensus_core::Hash64,
        opener: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwTokenizerArtifactInventoryProgressV1>, String> {
        self.virtual_processor.palw_tokenizer_artifact_inventory_v1_impl(class, opener)
    }

    fn palw_staged_flat_source_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedFlatProgressV1>, String> {
        self.virtual_processor.palw_staged_flat_source_v1_impl(key)
    }

    fn palw_staged_float_source_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedFloatProgressV1>, String> {
        self.virtual_processor.palw_staged_float_source_v1_impl(key)
    }

    fn palw_staged_float_pin_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        source: kaspa_consensus_core::Hash64,
        answerer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<kaspa_consensus_core::palw_float_constraint_source_v1::PalwFloatConstraintPinV1>, String> {
        self.virtual_processor.palw_staged_float_pin_v1_impl(key, source, answerer)
    }

    fn palw_staged_tir_view_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
        maximum_leaves: u64,
    ) -> Result<
        Option<(
            kaspa_consensus_core::palw_decode_view_v1::PalwDecodeViewV1,
            Box<kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>,
        )>,
        String,
    > {
        self.virtual_processor.palw_staged_tir_view_v1_impl(claim, maximum_leaves)
    }

    fn palw_staged_owned_native_seed_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_bundle_v1::PalwStagedNativeSeedContextV1>, String> {
        self.virtual_processor.palw_staged_owned_native_seed_v1_impl(key)
    }

    fn palw_staged_tir_prefix_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedTirPrefixProgressV1>, String> {
        self.virtual_processor.palw_staged_tir_prefix_v1_impl(key)
    }

    fn palw_tir_public_frame_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Result<Option<kaspa_consensus_core::palw_tir_public_claim_v1::PalwTirPublicFrameViewV1>, String> {
        self.virtual_processor.palw_tir_public_frame_v1_impl(claim)
    }

    fn palw_tir_public_conviction_preflight_v1(
        &self,
        filer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        claim: kaspa_consensus_core::Hash64,
        evidence: kaspa_consensus_core::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
    ) -> Result<Option<Result<(), String>>, String> {
        self.virtual_processor.palw_tir_public_conviction_preflight_v1_impl(filer, claim, evidence)
    }

    fn palw_tir_public_staged_conviction_preflight_v2(
        &self,
        filer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        claim: kaspa_consensus_core::Hash64,
        evidence: &kaspa_consensus_core::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
    ) -> Result<Option<Result<(), String>>, String> {
        self.virtual_processor.palw_tir_public_staged_conviction_preflight_v2_impl(filer, claim, evidence)
    }

    fn palw_tir_public_read_v1(
        &self,
        read: &mut dyn FnMut(&kaspa_consensus_core::palw_state_v2::PalwChainStateV2) -> Result<(), String>,
    ) -> Result<bool, String> {
        self.virtual_processor.palw_tir_public_read_v1_impl(read)
    }

    fn palw_tir_public_original_quote_v1(
        &self,
        original: kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        header: kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    ) -> Result<Option<(u64, kaspa_consensus_core::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1)>, String> {
        self.virtual_processor.palw_tir_public_original_quote_v1_impl(original, header)
    }

    fn palw_staged_tir_prefix_leaf_index_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        ordinal: u64,
        maximum_leaves: u64,
    ) -> Result<Option<u64>, String> {
        self.virtual_processor.palw_staged_tir_prefix_leaf_index_v1_impl(key, ordinal, maximum_leaves)
    }

    fn palw_staged_tir_layout_v1(
        &self,
        class_id: kaspa_consensus_core::Hash64,
    ) -> Option<kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1> {
        self.virtual_processor.palw_staged_tir_layout_v1_impl(class_id)
    }

    fn palw_staged_flat_pin_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        source: kaspa_consensus_core::Hash64,
        answerer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<kaspa_consensus_core::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1>, String> {
        self.virtual_processor.palw_staged_flat_pin_v1_impl(key, source, answerer)
    }

    fn palw_staged_resolved_public_context_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Result<
        Option<(
            kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
            kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        )>,
        String,
    > {
        self.virtual_processor.palw_staged_resolved_public_context_v1_impl(claim)
    }

    fn palw_staged_original_commitment_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Result<Option<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3>, String> {
        self.virtual_processor.palw_staged_original_commitment_v1_impl(claim)
    }

    fn palw_staged_history_recovery_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        seed: kaspa_consensus_core::Hash64,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedHistoryRecoveryProgressV1>, String> {
        self.virtual_processor.palw_staged_history_recovery_v1_impl(key, seed)
    }

    fn palw_v2_accepted_daa_v1(&self) -> Option<u64> {
        self.virtual_processor.palw_v2_accepted_daa_v1_impl()
    }

    fn palw_staged_custody_delegations_to_v1(
        &self,
        custodian: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<kaspa_consensus_core::Hash64>,
    ) -> Result<
        (
            Vec<(kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
            Option<kaspa_consensus_core::Hash64>,
        ),
        String,
    > {
        self.virtual_processor.palw_staged_custody_delegations_to_v1_impl(custodian, after)
    }

    fn palw_staged_history_grant_duty_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
        seat: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        receipt: kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwHistoryGrantDutyV1>, String> {
        self.virtual_processor.palw_staged_history_grant_duty_v1_impl(claim, seat, receipt)
    }

    fn palw_history_response_duties_v1(
        &self,
        signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(
            u64,
            kaspa_consensus_core::Hash64,
            kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
            kaspa_consensus_core::Hash64,
        )>,
    ) -> Result<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedHistoryDutyPageV1, String> {
        self.virtual_processor.palw_history_response_duties_v1_impl(signer, after)
    }

    fn palw_staged_public_context_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedPublicContextV1>, String> {
        self.virtual_processor.palw_staged_public_context_v1_impl(claim)
    }

    fn palw_staged_flat_duties_v1(
        &self,
        signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
    ) -> Result<
        (
            Vec<(kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
            Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
        ),
        String,
    > {
        self.virtual_processor.palw_staged_flat_duties_v1_impl(signer, after)
    }

    fn palw_staged_selected_duties_v1(
        &self,
        signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
    ) -> Result<
        (
            Vec<(kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
            Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
        ),
        String,
    > {
        self.virtual_processor.palw_staged_selected_duties_v1_impl(signer, after)
    }

    fn palw_staged_selected_events_v1(
        &self,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
    ) -> Result<Option<kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedSelectedProgressV1>, String> {
        self.virtual_processor.palw_staged_selected_events_v1_impl(key)
    }

    fn palw_staged_bundle_plan_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
        actual_raw_cap: Option<u64>,
    ) -> Result<Option<kaspa_consensus_core::palw_staged_bundle_v1::PalwStagedBundlePlanV1>, String> {
        self.virtual_processor.palw_staged_bundle_plan_v1_impl(claim, actual_raw_cap)
    }

    fn palw_prefix_response_duties_v1(
        &self,
        signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(u64, kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>,
    ) -> Result<kaspa_consensus_core::palw_staged_court_state_v1::PalwPrefixResponsePageV1, String> {
        self.virtual_processor.palw_prefix_response_duties_v1_impl(signer, after)
    }

    fn palw_prefix_page_write_v1(
        &self,
        snapshot: kaspa_consensus_core::BlockHash,
        key: (kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2),
        seed_id: kaspa_consensus_core::Hash64,
        opening: kaspa_consensus_core::palw_prefix_gdn_paged_v1::PalwGdnOwnedPageOpeningV1,
    ) -> Result<Vec<i32>, String> {
        self.virtual_processor.palw_prefix_page_write_v1_impl(snapshot, key, seed_id, opening)
    }

    fn palw_prefix_history_record_v1(
        &self,
        snapshot: kaspa_consensus_core::BlockHash,
        request: kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRequestV1,
    ) -> Result<Option<kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRecordV1>, String> {
        self.virtual_processor.palw_prefix_history_record_v1_impl(snapshot, request)
    }

    fn palw_prefix_history_record_current_v1(
        &self,
        request: kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRequestV1,
    ) -> Result<Option<kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRecordV1>, String> {
        self.virtual_processor.palw_prefix_history_record_current_v1_impl(request)
    }

    fn palw_staged_cache_gc_page_v1(
        &self,
        after: Option<(u64, kaspa_consensus_core::Hash64)>,
        limit: usize,
    ) -> Result<kaspa_consensus_core::palw_prefix_history_v1::PalwStagedCacheGcPageV1, String> {
        self.virtual_processor.palw_staged_cache_gc_page_v1_impl(after, limit)
    }

    fn palw_staged_cache_gc_ack_v1(
        &self,
        lease: kaspa_consensus_core::palw_prefix_history_v1::PalwStagedCacheLeaseV1,
    ) -> Result<bool, String> {
        self.virtual_processor.palw_staged_cache_gc_ack_v1_impl(lease)
    }

    fn palw_shared_claim_cache_gc_page_v1(
        &self,
        after: Option<(u64, kaspa_consensus_core::Hash64)>,
        limit: usize,
    ) -> Result<kaspa_consensus_core::palw_prefix_history_v1::PalwSharedClaimCacheGcPageV1, String> {
        self.virtual_processor.palw_shared_claim_cache_gc_page_v1_impl(after, limit)
    }

    fn palw_shared_claim_cache_gc_ack_v1(
        &self,
        lease: kaspa_consensus_core::palw_prefix_history_v1::PalwSharedClaimCacheLeaseV1,
    ) -> Result<bool, String> {
        self.virtual_processor.palw_shared_claim_cache_gc_ack_v1_impl(lease)
    }

    fn palw_rfc0016_report_v1(&self, class: Option<kaspa_consensus_core::Hash64>) -> Result<Option<(u64, bool, String)>, String> {
        self.virtual_processor.palw_rfc0016_report_v1_impl(class)
    }

    fn palw_claim_detail_v1(&self, claim: kaspa_consensus_core::Hash64) -> Result<Option<(u64, Option<String>)>, String> {
        self.virtual_processor.palw_user_view_v1_impl(|state, params, clock, anchor_delay| {
            kaspa_consensus_core::palw_user_views_v1::palw_claim_detail_json_v1(state, params, &claim, clock, anchor_delay)
                .map(|doc| doc.to_string())
        })
    }

    fn palw_court_state_v1(
        &self,
        target: kaspa_consensus_core::palw_user_views_v1::PalwCourtTargetV1,
    ) -> Result<Option<(u64, Option<String>)>, String> {
        self.virtual_processor.palw_user_view_v1_impl(|state, params, clock, _| {
            kaspa_consensus_core::palw_user_views_v1::palw_court_state_json_v1(state, params, target, clock).map(|doc| doc.to_string())
        })
    }

    fn palw_bond_detail_v1(
        &self,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<(u64, Option<String>)>, String> {
        self.virtual_processor.palw_user_view_v1_impl(|state, params, clock, _| {
            kaspa_consensus_core::palw_user_views_v1::palw_bond_detail_json_v1(state, params, &bond, clock).map(|doc| doc.to_string())
        })
    }

    fn palw_panel_da_disclosure_view_v1(&self, claim: kaspa_consensus_core::Hash64) -> Result<Option<(u64, Option<String>)>, String> {
        self.virtual_processor.palw_user_view_v1_impl(|state, _params, clock, _| {
            kaspa_consensus_core::palw_panel_da_disclosure_v1::palw_panel_da_disclosure_view_json_v1(state, &claim, clock)
                .map(|doc| doc.to_string())
        })
    }

    fn palw_router_binding_v2(
        &self,
        class: kaspa_consensus_core::Hash64,
    ) -> Result<Option<(u64, bool, Option<(u64, Vec<u8>)>)>, String> {
        self.virtual_processor.palw_router_binding_v2_impl(class)
    }

    fn palw_staged_preclaim_quote_v1(
        &self,
        original: kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        header: kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    ) -> Result<
        Option<(kaspa_consensus_core::BlockHash, u64, kaspa_consensus_core::palw_staged_preclaim_v1::PalwStagedPreclaimQuoteV1)>,
        String,
    > {
        self.virtual_processor.palw_staged_preclaim_quote_v1_impl(original, header)
    }

    fn palw_terminal_progress_v1(
        &self,
        key: kaspa_consensus_core::palw_terminal_carriage_v1::PalwTerminalKeyV1,
    ) -> Result<Option<kaspa_consensus_core::palw_terminal_carriage_v1::PalwTerminalProgressV1>, String> {
        self.virtual_processor.palw_terminal_progress_v1_impl(&key)
    }

    fn palw_legacy_answer_progress_v2(
        &self,
        key: kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwLegacyAnswerKeyV2,
    ) -> Result<Option<kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwLegacyAnswerProgressV2>, String> {
        self.virtual_processor.palw_legacy_answer_progress_v2_impl(&key)
    }

    fn palw_legacy_answer_history_v2(
        &self,
        block: kaspa_consensus_core::BlockHash,
        chunk: Box<kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwLegacyAnswerChunkV2>,
        maximum: u64,
    ) -> Result<Option<kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldAnswerCarriageV2>, String> {
        self.virtual_processor.palw_legacy_answer_history_v2_impl(block, &chunk, maximum)
    }

    fn palw_legacy_dispute_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwLegacyDisputeViewV1> {
        self.virtual_processor.palw_legacy_dispute_v1_impl(claim)
    }

    fn palw_fraud_filer_target_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Option<kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1> {
        kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(self.palw_state_v2_tip()?.as_ref(), &claim)
    }

    fn palw_gdn_step_candidates_v1(
        &self,
        me: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        after: Option<(u64, kaspa_consensus_core::Hash64)>,
        limit: usize,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwGdnStepCandidateV1> {
        self.virtual_processor.palw_gdn_step_candidates_v1_impl(me, after, limit)
    }

    fn palw_gdn_step_candidate_v1(
        &self,
        me: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        claim: kaspa_consensus_core::Hash64,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwGdnStepCandidateV1> {
        self.virtual_processor.palw_gdn_step_candidate_v1_impl(me, claim)
    }

    fn palw_gdn_court_limits_v2(
        &self,
        class_id: kaspa_consensus_core::Hash64,
    ) -> Option<kaspa_consensus_core::palw_gdn_court_v2::PalwGdnCourtLimitsV2> {
        self.virtual_processor.palw_gdn_court_limits_v2_impl(class_id)
    }

    fn palw_legacy_disputes_v1(
        &self,
        reserver: Option<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
        limit: usize,
    ) -> Vec<kaspa_consensus_core::Hash64> {
        self.virtual_processor.palw_legacy_disputes_v1_impl(reserver, limit)
    }

    fn palw_legacy_dispute_reservation_check_v1(
        &self,
        reservation: kaspa_consensus_core::palw_legacy_public_filer_v1::PalwDisputeReservationV1,
    ) -> Option<Result<u128, String>> {
        self.virtual_processor.palw_legacy_dispute_reservation_check_v1_impl(&reservation)
    }

    fn palw_fraud_filer_candidates_v1(
        &self,
        me: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwFraudFilerCandidateV1> {
        self.virtual_processor.palw_fraud_filer_candidates_v1_impl(&me)
    }

    fn palw_fraud_filer_candidates_for_claims_v1(
        &self,
        me: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        claims: Vec<kaspa_consensus_core::Hash64>,
    ) -> Vec<kaspa_consensus_core::palw_state_v2::PalwFraudFilerCandidateV1> {
        self.virtual_processor.palw_fraud_filer_candidates_for_claims_v1_impl(&me, &claims)
    }

    fn palw_fraud_filer_status_v1(
        &self,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwFraudFilerStatusV1> {
        self.virtual_processor.palw_fraud_filer_status_v1_impl(&bond)
    }

    fn palw_operator_da_candidates_v1(
        &self,
        operators: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> Vec<kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaCandidateV1> {
        self.virtual_processor.palw_operator_da_candidates_v1_impl(&operators)
    }

    fn palw_operator_da_standing_v1(
        &self,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaStandingV1> {
        self.virtual_processor.palw_operator_da_standing_v1_impl(&bond)
    }

    fn palw_capacity_audit_candidates_v1(
        &self,
        me: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Vec<kaspa_consensus_core::palw_audit_door_v1::PalwAuditCandidateV1> {
        self.virtual_processor.palw_capacity_audit_candidates_v1_impl(&me)
    }

    fn palw_reporter_filing_read_v1(
        &self,
        offence_key: kaspa_consensus_core::Hash64,
        commitment: kaspa_consensus_core::Hash64,
        reporter: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        gated: Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwReporterFilingReadV1> {
        self.virtual_processor.palw_reporter_filing_read_v1_impl(offence_key, commitment, reporter, gated)
    }

    fn palw_da_step_leaf_demand_check_v1(
        &self,
        claim: kaspa_consensus_core::Hash64,
        accuser: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        leaf: u64,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwDaStepLeafDemandCheckV1> {
        self.virtual_processor.palw_da_step_leaf_demand_check_v1_impl(claim, accuser, leaf)
    }

    fn palw_object_rehearsal_v1(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1> {
        self.virtual_processor.palw_object_rehearsal_v1_impl(object)
    }

    fn palw_claim_deadlines_v1(&self, claims: Vec<kaspa_consensus_core::Hash64>) -> Vec<(kaspa_consensus_core::Hash64, Option<u64>)> {
        self.virtual_processor.palw_claim_deadlines_v1_impl(&claims)
    }

    fn palw_readiness_urgency_v1(
        &self,
        carriers: &[kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1],
    ) -> Vec<Option<kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessUrgencyV1>> {
        self.virtual_processor.palw_readiness_urgency_v1_impl(carriers)
    }

    fn native_carriage_snapshot_v22(&self) -> Option<kaspa_consensus_core::palw_native_carrier_index_v22::NativeCarriageSnapshotV22> {
        self.virtual_processor.native_carriage_snapshot_v22()
    }

    fn palw_h1_carrier_refusals_v1(&self, txs: &[Arc<Transaction>]) -> Vec<Option<String>> {
        self.virtual_processor.palw_h1_carrier_refusals_v1_impl(txs)
    }

    fn palw_market_carrier_refusals_v1(&self, txs: &[Arc<Transaction>]) -> Vec<Option<String>> {
        self.virtual_processor.palw_market_carrier_refusals_v1_impl(txs)
    }

    fn palw_claim_readers_v2(&self, claim: kaspa_consensus_core::Hash64) -> Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2> {
        self.virtual_processor.palw_claim_readers_v2_impl(claim)
    }

    fn palw_false_valid_filing_check_v1(
        &self,
        object: kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Option<kaspa_consensus_core::palw_false_valid_filing_v1::PalwFalseValidFilingCheckV1> {
        self.virtual_processor.palw_false_valid_filing_check_v1_impl(&object)
    }

    fn palw_false_valid_relied_receipts_v1(
        &self,
        receipts: Vec<kaspa_consensus_core::palw_offence_attribution_v1::PalwFalseValidReceiptV1>,
    ) -> Option<Vec<bool>> {
        self.virtual_processor.palw_false_valid_relied_receipts_v1_impl(&receipts)
    }

    fn palw_receipt_pool_facts_v1(
        &self,
        claims: Vec<kaspa_consensus_core::Hash64>,
        bonds: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> Option<kaspa_consensus_core::palw_panel_v2::PalwReceiptPoolFactsV1> {
        self.virtual_processor.palw_receipt_pool_facts_v1_impl(&claims, &bonds)
    }

    fn palw_claim_roots_v2(
        &self,
        claim: kaspa_consensus_core::Hash64,
    ) -> Option<(kaspa_consensus_core::Hash64, kaspa_consensus_core::Hash64, u64)> {
        self.virtual_processor.palw_claim_roots_v2_impl(claim)
    }

    fn palw_tir_shard_plans_v1(
        &self,
    ) -> Vec<(kaspa_consensus_core::Hash64, kaspa_consensus_core::palw_tir_shard_v1::PalwTirShardPlanV1)> {
        self.virtual_processor.palw_tir_shard_plans_v1_impl()
    }

    fn palw_bond_payout_payload_v2(
        &self,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<kaspa_consensus_core::Hash64> {
        self.virtual_processor.palw_bond_payout_payload_v2_impl(&bond)
    }

    fn palw_bond_of_pubkey_v2(
        &self,
        pubkey: &[u8],
    ) -> Option<(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2)> {
        self.virtual_processor.palw_bond_of_pubkey_v2_impl(pubkey)
    }

    fn palw_claim_rows_v1(
        &self,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        role: kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1,
        include_terminal: bool,
        limit: usize,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwBondClaimsV1> {
        self.virtual_processor.palw_claim_rows_v1_impl(bond, role, include_terminal, limit, false)
    }

    fn palw_claim_rows_with_vesting_v1(
        &self,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        role: kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1,
        include_terminal: bool,
        limit: usize,
    ) -> Option<kaspa_consensus_core::palw_producer_v2::PalwBondClaimsV1> {
        self.virtual_processor.palw_claim_rows_v1_impl(bond, role, include_terminal, limit, true)
    }

    fn palw_vesting_v1(
        &self,
        query: kaspa_consensus_core::palw_vesting_read_v1::PalwVestingQueryV1,
        limit: usize,
        after: Option<(u64, kaspa_consensus_core::Hash64)>,
    ) -> Option<kaspa_consensus_core::palw_vesting_read_v1::PalwVestingReadV1> {
        self.virtual_processor.palw_vesting_v1_impl(query, limit, after)
    }

    fn palw_reward_vesting_v2(
        &self,
        query: kaspa_consensus_core::palw_vesting_read_v1::PalwVestingQueryV1,
        limit: usize,
        after: Option<(u64, kaspa_consensus_core::Hash64)>,
        exec_after: Option<kaspa_consensus_core::palw_reward_vesting_read_v2::PalwExecRewardCursorV2>,
    ) -> Option<kaspa_consensus_core::palw_reward_vesting_read_v2::PalwRewardVestingReadV2> {
        self.virtual_processor.palw_reward_vesting_v2_impl(query, limit, after, exec_after)
    }

    fn palw_capacity_shadow_v1(
        &self,
        options: kaspa_consensus_core::palw_capacity_shadow_v1::PalwCapacityShadowOptionsV1,
    ) -> Option<kaspa_consensus_core::palw_capacity_shadow_v1::PalwCapacityShadowV1> {
        self.virtual_processor.palw_capacity_shadow_v1_impl(options)
    }

    fn palw_panel_v3_observation_v1(
        &self,
        ids: Vec<kaspa_hashes::Hash64>,
        limit: usize,
    ) -> Option<kaspa_consensus_core::palw_permissionless_panel_v1::PanelV3ObservationV1> {
        self.virtual_processor.palw_panel_v3_observation_v1_impl(ids, limit)
    }

    fn palw_exec_v2_observation_v1(
        &self,
        roots: Vec<kaspa_hashes::Hash64>,
        block: Option<BlockHash>,
    ) -> Option<kaspa_consensus_core::palw_exec_v2_verify::PalwExecV2ObservationV1> {
        self.virtual_processor.palw_exec_v2_observation_v1_impl(roots, block)
    }

    fn palw_exec_v2_slice_statement_v1(
        &self,
        root: kaspa_hashes::Hash64,
        index: u32,
        claim: kaspa_hashes::Hash64,
        bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<kaspa_consensus_core::palw_exec_v2::PalwWorkSliceV1, String> {
        self.virtual_processor.palw_exec_v2_slice_statement_v1_impl(root, index, claim, bond)
    }

    fn palw_exec_v2_slice_relayable_v1(&self, slice: &kaspa_consensus_core::palw_exec_v2::PalwWorkSliceV1) -> bool {
        self.virtual_processor.palw_exec_v2_slice_relayable_v1_impl(slice)
    }

    fn palw_exec_v2_anchored_v1(&self, block: BlockHash) -> bool {
        self.virtual_processor.palw_exec_v2_anchored_v1_impl(block)
    }

    fn palw_v2_class_table(&self) -> Vec<kaspa_consensus_core::palw_state_v2::PalwClassRowV2> {
        self.virtual_processor.palw_v2_class_table_impl()
    }

    fn palw_class_census_v1(&self) -> Option<kaspa_consensus_core::palw_economic_compute_v1::PalwClassCensusReadV1> {
        self.virtual_processor.palw_class_census_v1_impl()
    }

    fn palw_claim_ledger_observations_v1(
        &self,
    ) -> Option<Vec<kaspa_consensus_core::palw_economics_ledger_v1::PalwClaimLedgerObservationV1>> {
        self.virtual_processor.palw_claim_ledger_observations_v1_impl()
    }

    fn palw_model_registry_v1(&self) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryReadV1> {
        self.virtual_processor.palw_model_registry_v1_impl()
    }

    fn palw_activation_pool_v1(
        &self,
        class_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::palw_activation_pool_v1::PalwActivationPoolReadV1> {
        self.virtual_processor.palw_activation_pool_v1_impl(class_id)
    }

    fn palw_panel_network_view_v1(&self) -> Option<kaspa_consensus_core::palw_panel_view_v1::PalwPanelNetworkViewV1> {
        self.virtual_processor.palw_panel_network_view_v1_impl()
    }

    fn palw_certified_families_v1(
        &self,
    ) -> Vec<(
        kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1,
        kaspa_hashes::Hash64,
        kaspa_consensus_core::palw_state_v2::PalwCertifiedFamilyStateV2,
    )> {
        self.virtual_processor.palw_certified_families_v1_impl()
    }

    fn palw_v2_registration_terms(&self) -> Option<kaspa_consensus_core::palw_state_v2::PalwRegistrationTermsV2> {
        self.virtual_processor.palw_v2_registration_terms_impl()
    }

    fn palw_registered_class_carriage_v1(
        &self,
        class_id: kaspa_hashes::Hash64,
    ) -> Option<(kaspa_consensus_core::palw_step::PalwShapeProfileV3, kaspa_consensus_core::palw_v2::PalwJobContextV2)> {
        self.virtual_processor.palw_registered_class_carriage_v1_impl(class_id)
    }

    fn palw_tir_class_record_v1(
        &self,
        class_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::palw_tir_admission_v1::PalwTirClassRecordV1> {
        self.palw_state_v2_tip()?.tir_class_v1(&class_id).cloned()
    }

    fn palw_kernel_route_v1(&self) -> Option<kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1> {
        self.palw_state_v2_tip()?.kernel_route().cloned()
    }

    fn palw_onboarding_v1(
        &self,
        class_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::palw_onboarding_v1::OnboardingReadV1> {
        self.palw_state_v2_tip()?.onboarding_read_v1(&class_id, self.get_virtual_daa_score())
    }

    fn palw_conformance_evidence_v1(
        &self,
        class_id: kaspa_hashes::Hash64,
    ) -> Option<kaspa_consensus_core::palw_onboarding_v1::ConformanceEvidenceReadV1> {
        self.palw_state_v2_tip()?.conformance_evidence_read_v1(&class_id, self.get_virtual_daa_score())
    }

    fn palw_adopt_class_carriage_v1(&self, class_id: kaspa_hashes::Hash64, carriage: &[u8]) -> Result<(), String> {
        self.virtual_processor.palw_adopt_class_carriage_v1_impl(class_id, carriage)
    }

    fn palw_improvement_open_epochs_v1(&self) -> Vec<kaspa_consensus_core::palw_improve_state_v1::PalwImprovementEpochViewV1> {
        self.palw_state_v2_tip().map(|state| state.improvement_open_epoch_views_v1()).unwrap_or_default()
    }

    fn palw_improvement_eval_views_v1(&self) -> Vec<kaspa_consensus_core::palw_improve_node_v1::PalwImprovementEvalViewV1> {
        self.palw_state_v2_tip().map(|state| state.improvement_eval_views_v1()).unwrap_or_default()
    }

    fn palw_improvement_status_v1(&self) -> kaspa_consensus_core::palw_improve_node_v1::PalwImprovementStatusV1 {
        self.palw_state_v2_tip().map(|state| state.improvement_status_v1()).unwrap_or_default()
    }

    fn palw_class_carriages_for_sync_v1(&self) -> Vec<(kaspa_hashes::Hash64, Vec<u8>)> {
        self.virtual_processor.palw_class_carriages_for_sync_v1_impl()
    }

    fn palw_native_snapshot_inventory_v1(
        &self,
        point: BlockHash,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeSnapshotInventoryV1,
        kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1,
    > {
        self.virtual_processor.palw_native_snapshot_inventory_v1_impl(point)
    }

    fn palw_native_snapshot_resource_bound_v1(
        &self,
        point: BlockHash,
    ) -> Option<kaspa_consensus_core::palw_native_resource_v1::NativeSnapshotResourceBoundV1> {
        self.virtual_processor.palw_native_snapshot_inventory_v1_impl(point).ok()?.physical_bound_v1().ok()
    }

    fn pruning_point_palw_state_shared_v1(
        &self,
        point: BlockHash,
    ) -> Option<std::sync::Arc<kaspa_consensus_core::palw_native_resource_v1::NativeStateCarriageV1>> {
        self.virtual_processor.pruning_point_palw_state_shared_v1_impl(point)
    }

    fn palw_class_carriages_for_snapshot_v1(
        &self,
        snapshot: &kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
        complete: bool,
    ) -> Result<Vec<(kaspa_hashes::Hash64, Vec<u8>)>, String> {
        self.virtual_processor.palw_class_carriages_for_snapshot_v1_impl(snapshot, complete)
    }

    fn palw_validate_snapshot_declarations_v1(
        &self,
        snapshot: &kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
        declarations: &[(kaspa_hashes::Hash64, Vec<u8>)],
        complete: bool,
    ) -> Result<(), String> {
        self.virtual_processor.palw_validate_snapshot_declarations_v1_impl(snapshot, declarations, complete)
    }

    fn palw_v2_pending_vertex_equivocations_v1(&self) -> Vec<kaspa_consensus_core::palw_vertex_v1::PalwVertexEquivocationV1> {
        self.virtual_processor.palw_v2_pending_vertex_equivocations_impl()
    }

    fn palw_v2_vertex_status_v1(
        &self,
        seat: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        from_round: u64,
    ) -> kaspa_consensus_core::palw_vertex_v1::PalwVertexStatusV1 {
        let state_params = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => &bundle.state,
            _ => return Default::default(),
        };
        self.palw_state_v2_tip().map(|state| state.vertex_status_v1(state_params, &seat, from_round)).unwrap_or_default()
    }

    fn palw_v2_mesh_status_v1(
        &self,
        seat: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> kaspa_consensus_core::palw_mesh_v1::PalwMeshStatusV1 {
        let state_params = match &self.config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => &bundle.state,
            _ => return Default::default(),
        };
        self.palw_state_v2_tip().map(|state| state.mesh_status_v1(state_params, &seat)).unwrap_or_default()
    }

    fn palw_v2_mesh_audit_duties_v1(
        &self,
        seat: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Vec<kaspa_consensus_core::palw_mesh_v1::PalwMeshAuditDutyV1> {
        let daa = self.get_virtual_daa_score();
        self.palw_state_v2_tip().map(|state| state.mesh_audit_duties_v1(&seat, daa)).unwrap_or_default()
    }

    fn palw_v2_mesh_audit_window_v1(&self, claim: kaspa_hashes::Hash64) -> Option<(u64, u64)> {
        self.palw_state_v2_tip().and_then(|state| state.mesh_audit_window_v1(&claim))
    }

    fn palw_disputable_claims_v2(
        &self,
        mine: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    ) -> Vec<kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2> {
        self.virtual_processor.palw_disputable_claims_v2_impl(&mine)
    }

    fn palw_court_close_verdict_v2(
        &self,
        session_id: &kaspa_consensus_core::Hash64,
        proof: &kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2> {
        self.virtual_processor.palw_court_close_verdict_v2_impl(session_id, proof)
    }

    fn palw_v2_receipt_quorum_assemble(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        self.virtual_processor.palw_v2_receipt_quorum_assemble_impl(claim, &candidates)
    }

    fn palw_v2_receipt_coverage_assemble(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        self.virtual_processor.palw_v2_receipt_coverage_assemble_impl(claim, &candidates)
    }

    fn palw_v2_batch_licence_assemble(
        &self,
        windows: Vec<kaspa_consensus_core::palw_batch_licence_v1::PalwSeatWindowV1>,
        due: Vec<kaspa_hashes::Hash64>,
        max_bytes: usize,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        self.virtual_processor.palw_v2_batch_licence_assemble_impl(&windows, &due, max_bytes)
    }

    fn palw_v2_tir_shard_part_assemble(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: Vec<kaspa_consensus_core::palw_tir_shard_v1::PalwSeatReceiptV4>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        self.virtual_processor.palw_v2_tir_shard_part_assemble_impl(claim, &candidates)
    }

    fn palw_v2_optimistic_assemble(
        &self,
        claim: kaspa_hashes::Hash64,
        candidates: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        self.virtual_processor.palw_v2_optimistic_assemble_impl(claim, &candidates)
    }

    fn palw_v2_supplementary_receipt_assemble(
        &self,
        claim: kaspa_hashes::Hash64,
        mine: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2> {
        self.virtual_processor.palw_v2_supplementary_receipt_assemble_impl(claim, &mine)
    }

    fn palw_v2_supplementary_assemble(
        &self,
        claim: kaspa_hashes::Hash64,
        v3_candidates: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV3>,
        v2_candidates: Vec<kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2>,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwSupplementaryOfferV1> {
        self.virtual_processor.palw_v2_supplementary_assemble_impl(claim, &v3_candidates, &v2_candidates)
    }

    fn import_pruning_point_palw_state(
        &self,
        pruning_point: BlockHash,
        carriage: kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
    ) -> PruningImportResult<()> {
        self.virtual_processor
            .with_pruning_import_session(None, |_| self.virtual_processor.import_pruning_point_palw_state(pruning_point, carriage))
    }

    fn pruning_point_palw_state(&self, pruning_point: BlockHash) -> Option<kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2> {
        self.virtual_processor.pruning_point_palw_state(pruning_point)
    }

    fn palw_state_proof_v1(
        &self,
        block: BlockHash,
        collection: &[u8],
    ) -> Result<(kaspa_consensus_core::header::Header, kaspa_consensus_core::palw_state_proof_v1::PalwFactProofV1), String> {
        self.virtual_processor.palw_state_proof_v1_impl(block, collection)
    }

    fn palw_native_execution_audit_v32(
        &self,
        block: BlockHash,
        job: [u8; 64],
    ) -> Result<
        (kaspa_consensus_core::header::Header, kaspa_consensus_core::palw_native_audit_v27::CheckedNativeExecutionAuditViewV27),
        String,
    > {
        self.virtual_processor.palw_native_execution_audit_v32(block, job)
    }

    fn palw_native_coinbase_proof_v33(
        &self,
        block: BlockHash,
    ) -> Result<(kaspa_consensus_core::header::Header, kaspa_consensus_core::palw_native_redemption_v33::NativeCoinbaseProofV33), String>
    {
        self.virtual_processor.palw_native_coinbase_proof_v33(block)
    }

    fn palw_fork_choice_openings_v1(
        &self,
        blocks: &[BlockHash],
    ) -> Result<kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceServedV1, String> {
        let sink = self.get_sink();
        let mut tips = self.get_tips();
        tips.sort();
        self.virtual_processor.palw_fork_choice_openings_v1_impl(blocks, sink, tips, &Default::default())
    }

    fn palw_fork_choice_openings_v2(
        &self,
        blocks: &[BlockHash],
        extras: &kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceExtrasRequestV2,
    ) -> Result<kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceServedV1, String> {
        let sink = self.get_sink();
        let mut tips = self.get_tips();
        tips.sort();
        self.virtual_processor.palw_fork_choice_openings_v1_impl(blocks, sink, tips, extras)
    }

    fn import_pruning_point_overlay_snapshot(
        &self,
        pruning_point: BlockHash,
        snapshot: kaspa_consensus_core::dns_finality::OverlaySnapshot,
    ) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(None, |_| {
            self.virtual_processor.import_pruning_point_overlay_snapshot(pruning_point, snapshot)
        })
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
        self.services.reachability_service.try_is_chain_ancestor_of(low, high)
            .map_err(|e| ConsensusError::GeneralOwned(format!("native chain reachability read: {e}")))
    }

    // max_blocks has to be greater than the merge set size limit
    fn get_hashes_between(&self, low: BlockHash, high: BlockHash, max_blocks: usize) -> ConsensusResult<(Vec<BlockHash>, BlockHash)> {
        let _guard = self.pruning_lock.blocking_read();
        assert!(max_blocks as u64 > self.services.sync_manager.mergeset_capacity_for(high));
        self.validate_block_exists(low)?;
        self.validate_block_exists(high)?;

        Ok(self.services.sync_manager.antipast_hashes_between(low, high, Some(max_blocks)))
    }

    fn get_header(&self, hash: BlockHash) -> ConsensusResult<Arc<Header>> {
        self.headers_store.get_header(hash).optional()
            .map_err(|e| ConsensusError::GeneralOwned(format!("native header read: {e}")))?
            .ok_or(ConsensusError::HeaderNotFound(hash))
    }

    fn get_header_download_hint(&self) -> BlockHash {
        self.headers_selected_tip_store.read().get().unwrap().hash
    }

    /// Unit D, site 2: the order of this consensus's own virtual sink — the chain it would
    /// actually keep, not the chain it happened to download.
    ///
    /// **Except where there is no virtual sink at all**, which is every STAGING consensus and
    /// therefore every headers-proof IBD. Staging is built `skip_adding_genesis`, so `get_sink()`
    /// answers the all-zero hash and this returned `None` — which
    /// `validate_staging_palw_order` turns into a hard `ProtocolError`, refusing the sync against
    /// every peer forever. `palw_weighing_point_v2` supplies the answer that IS available there:
    /// the pruning point whose PALW carriage the flow imported and root-verified one line earlier.
    fn get_palw_candidate_order_v2(&self) -> Option<kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1> {
        self.get_palw_candidate_order_v2_checked().ok().flatten()
    }

    fn get_palw_candidate_order_v2_checked(
        &self,
    ) -> Result<
        Option<kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1>,
        kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1,
    > {
        let Some(point) = self.virtual_processor.palw_weighing_point_v2_checked(self.get_sink())? else { return Ok(None) };
        self.virtual_processor.palw_candidate_order_v2_checked(point)
    }

    /// Same authenticated weighing point for the IBD exclusive-claim comparison.
    fn get_palw_rule_e_weighing_v1(&self) -> Option<kaspa_consensus_core::palw_fork_choice_rule_e_v1::PalwRuleEWeighingV1> {
        self.get_palw_rule_e_weighing_v1_checked().ok().flatten()
    }

    fn get_palw_rule_e_weighing_v1_checked(
        &self,
    ) -> Result<
        Option<kaspa_consensus_core::palw_fork_choice_rule_e_v1::PalwRuleEWeighingV1>,
        kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1,
    > {
        let Some(point) = self.virtual_processor.palw_weighing_point_v2_checked(self.get_sink())? else { return Ok(None) };
        let Some(state) = self.virtual_processor.palw_candidate_state_for_selection_v2(point)? else { return Ok(None) };
        let Ok(daa_score) = self.headers_store.get_daa_score(point) else { return Ok(None) };
        Ok(Some(kaspa_consensus_core::palw_fork_choice_rule_e_v1::PalwRuleEWeighingV1 { state, block: point, daa_score }))
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
                .iter()
                .enumerate()
                .filter(|(index, _tx)| indices.contains(&(*index as TransactionIndexType)))
                .map(|(_, tx)| tx.clone())
                .collect::<Vec<_>>();

            if res.len() != indices.len() {
                Err(ConsensusError::TransactionIndexOutOfBounds(*indices.iter().max().unwrap(), tx_len, hash))
            } else {
                Ok(res)
            }
        } else {
            Ok(transactions.as_ref().clone())
        }
    }

    fn get_block_body(&self, hash: BlockHash) -> ConsensusResult<kaspa_consensus_core::block::BlockTransactionsV3> {
        if match self.statuses_store.read().get(hash).optional().unwrap() {
            Some(status) => !status.has_block_body(),
            None => true,
        } {
            return Err(ConsensusError::BlockNotFound(hash));
        }

        self.block_transactions_store.get(hash).optional().unwrap().ok_or(ConsensusError::BlockNotFound(hash))
    }

    fn get_evm_tx_locations(&self, tx_hash: kaspa_hashes::EvmH256) -> ConsensusResult<kaspa_consensus_core::evm::EvmTxLocations> {
        self.storage.evm_tx_index_store.get_or_default(tx_hash)
            .map_err(|e| ConsensusError::GeneralOwned(format!("EVM transaction locations read: {e}")))
    }

    fn get_evm_tx_receipt(
        &self,
        tx_hash: kaspa_hashes::EvmH256,
    ) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmTxReceiptView>> {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmReceiptsStoreReader};
        let unavailable = |e| ConsensusError::GeneralOwned(format!("EVM receipt read: {e}"));
        let row = self.storage.evm_tx_index_store.get_or_default(tx_hash).map_err(unavailable)?;
        for (accepting, receipt_index) in row.accepted_in.iter().rev() {
            // Canonical resolution: only an acceptance on the CURRENT selected
            // chain counts (§16 — orphaned receipts read as null at `latest`).
            match self.is_chain_block(*accepting) {
                Ok(true) => {}
                Ok(false) | Err(ConsensusError::HeaderNotFound(_)) => continue,
                Err(error) => return Err(error),
            }
            let receipts = self.storage.evm_receipts_store.get(*accepting).optional().map_err(unavailable)?.unwrap_or_default();
            let idx = *receipt_index as usize;
            if idx >= receipts.receipts.len() || receipts.tx_hashes.get(idx) != Some(&tx_hash) {
                continue; // defensive: index row out of sync with the receipts row
            }
            let evm_number =
                self.storage.evm_header_store.get(*accepting).optional().map_err(unavailable)?.map(|h| h.evm_number).unwrap_or_default();
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
        self.storage.evm_header_store.get(block).optional()
            .map_err(|e| ConsensusError::GeneralOwned(format!("EVM execution header read: {e}")))
    }

    fn get_native_settlement_snapshot(
        &self,
    ) -> ConsensusResult<Option<kaspa_consensus_core::palw_native_settlement_v1::NativeSettlementSnapshotV1>> {
        let sink = self.get_sink();
        if !self.pruning_meta_stores.read().pruning_utxoset_stable_flag() {
            return Err(ConsensusError::General("pruning import is incomplete; native settlement is unavailable"));
        }
        let Some(retirement) =
            self.config.params.palw_dns_retirement.filter(|r| r.activation.is_active(self.get_sink_daa_score_timestamp().daa_score))
        else {
            return Ok(None);
        };
        self.virtual_processor.native_finalized_publication().map_err(|()| {
            ConsensusError::General("native finalized publication is incompatible or unreadable; reconstruct or resync")
        })?;
        let snapshot = self
            .storage
            .evm_heads_store
            .read()
            .native_snapshot()
            .optional()
            .map_err(|e| ConsensusError::GeneralOwned(format!("native settlement snapshot: {e}")))?
            .flatten();
        if let Some(s) = &snapshot {
            if s.version != 1
                || s.generation != sink
                || s.policy_id != retirement.settlement.id()
                || s.ruleset_id != self.virtual_processor.native_boot_ruleset_id()
                || s.retirement_daa != retirement.activation.daa_score()
            {
                return Err(ConsensusError::General("native settlement generation/ruleset mismatch; reconstruct or resync"));
            }
            // The public evidence API and ETH tags share the same root/provenance checks.
            use crate::model::stores::evm::EvmHeaderStoreReader;
            for block in [s.latest, s.safe, s.finalized].into_iter().flatten() {
                let execution = self.storage.evm_header_store.get(block).map_err(|_| ConsensusError::MissingData(block))?;
                let header = self.headers_store.get_header(block).map_err(|_| ConsensusError::MissingData(block))?;
                if header.evm_commitment_root != execution.commitment_root()
                    || !self.services.reachability_service.try_is_chain_ancestor_of(block, sink).unwrap_or(false)
                {
                    return Err(ConsensusError::General("native settlement lacks a canonical, root-verified result"));
                }
            }
            if s.safe.is_some_and(|safe| {
                s.latest
                    .is_none_or(|latest| !self.services.reachability_service.try_is_chain_ancestor_of(safe, latest).unwrap_or(false))
            }) || s.finalized.is_some_and(|finalized| {
                s.safe
                    .is_none_or(|safe| !self.services.reachability_service.try_is_chain_ancestor_of(finalized, safe).unwrap_or(false))
            }) {
                return Err(ConsensusError::General("native settlement heads are not an ordered executed prefix"));
            }
        }
        Ok(snapshot)
    }

    fn get_native_safe_readiness(
        &self,
    ) -> ConsensusResult<Option<kaspa_consensus_core::palw_native_readiness_v1::NativeSafeReadinessV1>> {
        // The same gate as the snapshot: nothing before the retirement fence is active at the sink.
        if self
            .config
            .params
            .palw_dns_retirement
            .is_none_or(|r| !r.activation.is_active(self.get_sink_daa_score_timestamp().daa_score))
        {
            return Ok(None);
        }
        Ok(self.virtual_processor.native_safe_readiness(self.get_sink()))
    }

    fn get_evm_canonical_heads(&self) -> ConsensusResult<Option<kaspa_consensus_core::evm::CanonicalEvmHeads>> {
        use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader};
        if !self.pruning_meta_stores.read().pruning_utxoset_stable_flag() {
            return Err(ConsensusError::General("pruning import is incomplete; canonical EVM heads are unavailable"));
        }
        let mut heads = self
            .storage
            .evm_heads_store
            .read()
            .get()
            .optional()
            .map_err(|e| ConsensusError::GeneralOwned(format!("EVM canonical heads: {e}")))?;
        if self.config.params.palw_dns_retired_at(self.get_sink_daa_score_timestamp().daa_score) {
            let snapshot = self.get_native_settlement_snapshot()?;
            if let Some(h) = &mut heads {
                match snapshot {
                    None => {
                        h.safe = BlockHash::default();
                        h.finalized = BlockHash::default();
                    }
                    Some(s) if h.latest_head() == s.latest && h.safe_head() == s.safe && h.finalized_head() == s.finalized => {}
                    Some(_) => {
                        return Err(ConsensusError::General(
                            "EVM heads and native settlement snapshot disagree; reconstruct or resync",
                        ));
                    }
                }
                let sink = self.get_sink();
                for block in [h.latest_head(), h.safe_head(), h.finalized_head()].into_iter().flatten() {
                    let execution = self.storage.evm_header_store.get(block).map_err(|_| ConsensusError::MissingData(block))?;
                    let header = self.headers_store.get_header(block).map_err(|_| ConsensusError::MissingData(block))?;
                    if header.evm_commitment_root != execution.commitment_root()
                        || !self.services.reachability_service.try_is_chain_ancestor_of(block, sink).unwrap_or(false)
                    {
                        return Err(ConsensusError::General("EVM settlement head lacks a canonical, root-verified result"));
                    }
                }
                if h.safe_head().is_some_and(|safe| {
                    h.latest_head().is_none_or(|latest| {
                        !self.services.reachability_service.try_is_chain_ancestor_of(safe, latest).unwrap_or(false)
                    })
                }) || h.finalized_head().is_some_and(|finalized| {
                    h.safe_head().is_none_or(|safe| {
                        !self.services.reachability_service.try_is_chain_ancestor_of(finalized, safe).unwrap_or(false)
                    })
                }) {
                    return Err(ConsensusError::General("EVM finalized/safe/latest are not an ordered executed prefix"));
                }
            }
        }
        Ok(heads)
    }

    fn get_evm_state_snapshot_of(&self, block: BlockHash) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmStateSnapshot>> {
        use crate::model::stores::evm::EvmStateStoreReader;
        // Hot path: the per-block 206 snapshot (present on every node that persists it — the default).
        if let Some(snapshot) = self.storage.evm_state_store.get(block).optional()
            .map_err(|e| ConsensusError::GeneralOwned(format!("EVM hot snapshot read: {e}")))? {
            return Ok(Some(snapshot));
        }
        // C-01 S9b: 206 was retired (--evm-retire-206) or this block was committed while retired. Serve
        // the state from the flat backend instead — materialize it directly when `block` is the flat
        // canonical head (exact, O(state)), else §12-reconstruct (root-verified). This keeps eth_call /
        // trace / account reads working without the 206 store. Read-path only; behavior-preserving when
        // 206 is present (returned above) and on inert/non-EVM nets (no flat head ⇒ reconstruct ⇒ None).
        #[cfg(feature = "evm")]
        if let Some(ptr) = self.storage.evm_latest_state_ptr_store.read().get()
            .map_err(|e| ConsensusError::GeneralOwned(format!("EVM flat state pointer read: {e}")))?
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

    fn evm_activation_fences(&self) -> (u64, u64, u64, u64) {
        (
            self.config.params.evm_gas_pool_v2_activation_daa_score,
            self.config.params.evm_f002_withdraw_cap_activation_daa_score,
            self.config.params.evm_bridge_ledger_activation_daa_score,
            self.config.params.evm_f003_mldsa_verify_activation_daa_score,
        )
    }

    fn reconstruct_evm_state_at(&self, block: BlockHash) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmStateSnapshot>> {
        use crate::model::stores::evm::EvmHeaderStoreReader;
        use kaspa_consensus_core::errors::consensus::ConsensusError;

        // Not an EVM block (no committed header) ⇒ None — distinct from "EVM block
        // whose state history this node doesn't retain", which is an Err below.
        let Some(target_header) = self.storage.evm_header_store.get(block).optional()
            .map_err(|e| ConsensusError::GeneralOwned(format!("EVM reconstruction target header read: {e}")))? else {
            return Ok(None);
        };

        #[cfg(feature = "evm")]
        {
            use crate::model::stores::evm::{EvmStateCheckpointStoreReader, EvmStateDiffStoreReader};
            let oops = |m: String| ConsensusError::GeneralOwned(m);

            // Walk `block`'s selected-parent chain backward (design §12.4) to the
            // nearest checkpoint (its full state) or the pre-activation genesis,
            // collecting the forward diffs to replay. Pure store-walk. Pre-activation
            // is judged by the L1 DAA score vs the activation score — never by EVM-row
            // presence, which pruning erases (the t10 empty-seed bug).
            let (seed, forward_diffs) = crate::processes::evm::gather_reconstruction_inputs(
                block,
                |b| self.storage.evm_state_checkpoint_store.get(b),
                |b| self.storage.evm_state_diff_store.get(b),
                |b| {
                    self.storage
                        .headers_store
                        .get_compact_header_data(b)
                        .map(|c| c.daa_score < self.config.params.evm_activation_daa_score)
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
        let unavailable = |e| ConsensusError::GeneralOwned(format!("EVM block read: {e}"));
        let Some(header) = self.storage.evm_header_store.get(l1_hash).optional().map_err(unavailable)? else { return Ok(None) };
        let tx_hashes = self.storage.evm_receipts_store.get(l1_hash).optional().map_err(unavailable)?
            .map(|r| r.tx_hashes).unwrap_or_default();
        // RPC §7.3 size: absent immutable raw rows retain their existing zero contribution.
        let mut encoded_size = 0;
        for h in &tx_hashes {
            encoded_size += self.storage.evm_raw_tx_store.get(*h).map_err(unavailable)?
                .map(|r| r.raw.len() as u64).unwrap_or(0);
        }
        Ok(Some(kaspa_consensus_core::evm::EvmBlockResponse { header, l1_hash, tx_hashes, encoded_size }))
    }

    fn get_evm_parent_l1_hash(&self, l1_hash: BlockHash) -> ConsensusResult<Option<BlockHash>> {
        use crate::model::stores::evm::EvmHeaderStoreReader;
        use kaspa_consensus_core::evm::EVM_GENESIS_STATE_ROOT;
        let unavailable = |e| ConsensusError::GeneralOwned(format!("EVM exact parent read: {e}"));
        let child = self.storage.evm_header_store.get(l1_hash).optional().map_err(unavailable)?
            .ok_or(ConsensusError::MissingData(l1_hash))?;
        if child.evm_number == 0 {
            return Ok(None);
        }
        let parent = self.ghostdag_store.get_selected_parent(l1_hash).map_err(unavailable)?;
        if let Some(header) = self.storage.evm_header_store.get(parent).optional().map_err(unavailable)? {
            if header.evm_number.checked_add(1) != Some(child.evm_number) || header.state_root != child.parent_state_root {
                return Err(ConsensusError::General("immutable EVM parent number/state-root mismatch"));
            }
            return Ok(Some(parent));
        }
        // The first execution has no preceding EVM header, only the native
        // genesis or pre-activation selected parent. Missing post-activation
        // history must not masquerade as that boundary or a zero parentHash.
        let first = parent == self.config.params.genesis.hash
            || self.headers_store.get_daa_score(parent).map_err(unavailable)? < self.config.params.evm_activation_daa_score;
        if first && child.evm_number == 1 && child.parent_state_root == EVM_GENESIS_STATE_ROOT {
            Ok(None)
        } else {
            Err(ConsensusError::MissingData(parent))
        }
    }

    fn get_evm_block_logs(&self, l1_hash: BlockHash) -> ConsensusResult<Vec<kaspa_consensus_core::evm::EvmLogEntry>> {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmReceiptsStoreReader};
        let unavailable = |e| ConsensusError::GeneralOwned(format!("EVM immutable logs read: {e}"));
        // Read by L1 hash from the IMMUTABLE header + receipts stores (never the
        // reorg-mutable number map): the §9 logs reorg pump emits detached blocks,
        // which are no longer canonical but whose receipts are still stored. No
        // canonical filter here — the pump tags removed=true/false itself.
        let Some(header) = self.storage.evm_header_store.get(l1_hash).optional().map_err(unavailable)? else { return Ok(Vec::new()) };
        let receipts = self.storage.evm_receipts_store.get(l1_hash).optional().map_err(unavailable)?.unwrap_or_default();
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

    fn get_evm_block_logs_by_rpc_hash(
        &self,
        rpc_hash: kaspa_hashes::EvmH256,
    ) -> ConsensusResult<Option<Vec<kaspa_consensus_core::evm::EvmLogEntry>>> {
        use crate::model::stores::evm::{EvmBlockHashMapStoreReader, EvmHeaderStoreReader};
        let unavailable = |e| ConsensusError::GeneralOwned(format!("EVM hash logs read: {e}"));
        let Some(block) = self.storage.evm_block_hash_map_store.get(rpc_hash).map_err(unavailable)? else { return Ok(None) };
        if block.as_bytes()[..32] != rpc_hash.as_bytes()[..] {
            return Err(ConsensusError::GeneralOwned("EVM RPC hash index names a different block hash".into()));
        }
        if self.storage.evm_header_store.get(block).optional().map_err(unavailable)?.is_none() {
            return Ok(None);
        }
        // Exact immutable identity, intentionally not the reorg-mutable number
        // index or canonical-only range reader. EIP-234 includes retained forks.
        self.get_evm_block_logs(block).map(Some)
    }

    fn get_evm_raw_tx(&self, tx_hash: kaspa_hashes::EvmH256) -> ConsensusResult<Option<Vec<u8>>> {
        use crate::model::stores::evm::EvmRawTxStoreReader;
        self.storage.evm_raw_tx_store.get(tx_hash)
            .map(|row| row.map(|r| r.raw))
            .map_err(|e| ConsensusError::GeneralOwned(format!("EVM raw transaction read: {e}")))
    }

    fn get_evm_block_by_number(&self, evm_number: u64) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmBlockResponse>> {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmNumberStoreReader};
        let unavailable = |e| ConsensusError::GeneralOwned(format!("EVM block selector read: {e}"));
        // Resolve the (upsert) number index, then re-validate canonicality: the
        // candidate must still be a selected-chain block AND its header's
        // evm_number must match (a reorg-orphaned row reads as absent — the same
        // canonical-resolution guard as `get_evm_tx_receipt`).
        let Some(l1_hash) = self.storage.evm_number_store.get(evm_number).map_err(unavailable)? else { return Ok(None) };
        if !self.is_chain_block(l1_hash)? {
            return Ok(None);
        }
        match self.storage.evm_header_store.get(l1_hash).optional().map_err(unavailable)? {
            Some(h) if h.evm_number == evm_number => self.get_evm_block_by_l1_hash(l1_hash),
            _ => Ok(None),
        }
    }

    fn get_evm_block_by_rpc_hash(
        &self,
        rpc_hash: kaspa_hashes::EvmH256,
    ) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmBlockResponse>> {
        use crate::model::stores::evm::EvmBlockHashMapStoreReader;
        let Some(l1_hash) = self.storage.evm_block_hash_map_store.get(rpc_hash)
            .map_err(|e| ConsensusError::GeneralOwned(format!("EVM RPC hash read: {e}")))? else { return Ok(None) };
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
        let unavailable = |e| ConsensusError::GeneralOwned(format!("EVM range logs read: {e}"));
        let canonical = |block| match self.is_chain_block(block) {
            Ok(value) => Ok(value),
            Err(ConsensusError::HeaderNotFound(_)) => Ok(false),
            Err(error) => Err(error),
        };
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
        let indexed_floor = if addresses.is_empty() { None } else {
            self.storage.evm_log_index_store.try_indexed_floor().map_err(unavailable)?
        };
        if indexed_floor.is_some_and(|f| from_number >= f) {
            let mut out: Vec<kaspa_consensus_core::evm::EvmLogEntry> = Vec::new();
            let mut seen: std::collections::HashSet<[u8; 20]> = std::collections::HashSet::new();
            for addr in addresses.iter().copied() {
                if !seen.insert(addr.as_bytes()) {
                    continue; // a log has one address — dedup duplicate seeds
                }
                // Collect this address's in-range postings (ascending block order),
                // then resolve each (the iterator borrows the store).
                let mut locs = Vec::new();
                for loc in self.storage.evm_log_index_store
                    .try_bucket_locs(kaspa_consensus_core::evm::LogPostingKind::Address, &addr.as_bytes()) {
                    let loc = loc.map_err(unavailable)?;
                    if loc.evm_number < from_number { continue; }
                    if loc.evm_number > to_number { break; }
                    locs.push(loc);
                }
                for loc in locs {
                    // Canonical-resolve the posting (drop side-branch entries) — the
                    // same backstop get_evm_block_by_number uses.
                    if !canonical(loc.l1_hash)? {
                        continue;
                    }
                    let Some(header) = self.storage.evm_header_store.get(loc.l1_hash).optional().map_err(unavailable)? else { continue };
                    if header.evm_number != loc.evm_number {
                        continue;
                    }
                    let receipts = self.storage.evm_receipts_store.get(loc.l1_hash).optional().map_err(unavailable)?.unwrap_or_default();
                    let Some(receipt) = receipts.receipts.get(loc.tx_index as usize) else { continue };
                    let Some(log) = receipt.logs.get(loc.in_receipt_log_index as usize) else { continue };
                    if log.address != addr {
                        continue;
                    }
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
            let Some(l1_hash) = self.storage.evm_number_store.get(n).map_err(unavailable)? else { continue };
            // Reorg-validate the (upsert) number index before trusting the row.
            if !canonical(l1_hash)? {
                continue;
            }
            let Some(header) = self.storage.evm_header_store.get(l1_hash).optional().map_err(unavailable)? else { continue };
            if header.evm_number != n {
                continue;
            }
            let receipts = self.storage.evm_receipts_store.get(l1_hash).optional().map_err(unavailable)?.unwrap_or_default();
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
    fn clear_pruning_utxo_set(&self) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(None, |_| self.virtual_processor.clear_pruning_import_utxos(None))
    }

    fn clear_pruning_utxo_set_for_session(&self, session: PruningImportSessionV1) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(Some(session), |state| {
            if state.utxos_started || state.utxos_imported {
                return Err(PruningImportError::PruningImportSessionRefused {
                    point: session.point,
                    epoch: session.epoch,
                    reason: "the UTXO stream has already started; begin a new session to retry".into(),
                });
            }
            let mut state = state.clone();
            state.utxos_started = true;
            self.virtual_processor.clear_pruning_import_utxos(Some(&state))
        })
    }

    /// The usual flow consists of the pruning point naturally updating during pruning, and hence maintains consistency by default
    /// During pruning catchup, we need to manually update the pruning point and
    /// make sure that consensus looks "as if" it has just moved to a new pruning point.
    fn intrusive_pruning_point_update(&self, new_pruning_point: BlockHash, syncer_sink: BlockHash) -> ConsensusResult<()> {
        let pruning_points_to_add = self.get_and_verify_path_to_new_pruning_point(new_pruning_point, syncer_sink)?;

        // If all has gone well, we can finally update pruning point and other stores.
        self.intrusive_pruning_point_store_writes(new_pruning_point, syncer_sink, pruning_points_to_add)
    }

    fn complete_pruning_point_import(&self, point: BlockHash) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(None, |_| self.virtual_processor.complete_pruning_point_import(point))
    }

    fn complete_pruning_point_import_for_session(&self, session: PruningImportSessionV1) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(Some(session), |state| {
            if !state.utxos_started || !state.utxos_imported {
                return Err(PruningImportError::PruningImportSessionRefused {
                    point: session.point,
                    epoch: session.epoch,
                    reason: "UTXO stream has not finalized".into(),
                });
            }
            self.virtual_processor.complete_pruning_point_import(session.point)
        })
    }

    fn set_pruning_utxoset_stable_flag(&self, val: bool) -> PruningImportResult<()> {
        self.virtual_processor.with_pruning_import_session(None, |_| {
            let mut meta = self.pruning_meta_stores.write();
            let mut batch = rocksdb::WriteBatch::default();
            meta.set_pruning_utxoset_stable_flag(&mut batch, val).unwrap();
            self.virtual_processor.write_pruning_import_barrier(batch);
            Ok(())
        })
    }

    fn is_pruning_utxoset_stable(&self) -> bool {
        let pruning_meta_read = self.pruning_meta_stores.read();
        pruning_meta_read.pruning_utxoset_stable_flag()
    }

    fn is_pruning_point_anticone_fully_synced(&self) -> bool {
        let pruning_meta_read = self.pruning_meta_stores.read();
        pruning_meta_read.is_anticone_fully_synced()
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

/// **ADR-0078 Decision 5: what a consumer may read off the chain about one claim's derivations.**
///
/// The arithmetic behind [`ConsensusApi::palw_derived_artifacts_v1`], separated from the tip read
/// so the acceptance test can ask a folded state the same question the RPC asks a materialized
/// one. Three answers in one, because a derivation is only checkable next to the claim it names:
///
/// * the claim, for the `output_root` X6 recomputes against, the phase (a derivation of a claim
///   that later voided is a derivation of a voided claim, "and says so when read"), and the block
///   that accepted the claim;
/// * the executor's registered bond key — what the object's `executor_pubkey` had to equal to be
///   accepted, so a reader learns whose name is on the provenance without a second lookup. Empty
///   for a claim whose bond has since retired: the claim outlives the bond record, and an empty
///   key is the honest statement of that rather than a refusal to answer;
/// * the derived rows under this claim, in transformer-id order (the table's own order), each
///   still paired with its key so `transformer_id` — which the row does not repeat — is on the
///   wire.
///
/// `None` only when the chain does not have the claim. What it never does is resolve
/// `grammar_id`, `transformer_id` or `kind`: consensus holds no registry (see
/// `check_derived_shape_v1`'s note on SA-5), and a read that pretended otherwise would be
/// inventing meaning the transition never checked.
#[allow(clippy::type_complexity)]
pub fn palw_derived_claim_view_v1(
    state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
    claim_id: kaspa_hashes::Hash64,
) -> Option<(
    kaspa_consensus_core::palw_state_v2::PalwClaimStateV2,
    Vec<u8>,
    Vec<(kaspa_consensus_core::palw_derived_v1::PalwDerivedKeyV1, kaspa_consensus_core::palw_derived_v1::PalwDerivedRowV1)>,
)> {
    let claim = state.claim(&claim_id)?.clone();
    let executor_pubkey = state.bond(&claim.bond).map(|b| b.pubkey.clone()).unwrap_or_default();
    let rows = state.derived_artifacts_of(claim_id).map(|(key, row)| (*key, row.clone())).collect();
    Some((claim, executor_pubkey, rows))
}

impl Consensus {
    /// Missing reserve rows encode zero only for a retained, UTXO-validated
    /// block (or genesis). A matching imported snapshot also establishes the
    /// balance. Unknown/pruned history remains unknown; storage errors remain
    /// errors for the checked acceptance reader.
    fn dns_reserve_balance_checked_v1(&self, block: BlockHash) -> ConsensusResult<Option<u64>> {
        use crate::model::stores::pruning_overlay_snapshot::PruningPointOverlaySnapshotStoreReader;
        let unavailable = |e| ConsensusError::GeneralOwned(format!("DNS reserve read: {e}"));
        // Candidate commit stages acceptance/reserve cache writes before the DB
        // batch is durable. Its status write guard stays exclusive through that
        // commit; acquire it first so staged rows cannot establish a post-state.
        let statuses = self.statuses_store.read();
        if statuses.get(block).optional().map_err(unavailable)? == Some(BlockStatus::StatusUTXOValid) {
            if let Some(balance) = self.storage.reserve_balance_store.get(block).optional().map_err(unavailable)? {
                return Ok(Some(balance));
            }
            if block == self.config.params.genesis.hash
                || self.acceptance_data_store.get(block).optional().map_err(unavailable)?.is_some()
            {
                return Ok(Some(0));
            }
        }
        drop(statuses);
        Ok(self
            .storage
            .pruning_overlay_snapshot_store
            .read()
            .get()
            .optional()
            .map_err(unavailable)?
            .filter(|snapshot| snapshot.pruning_point == block)
            .map(|snapshot| snapshot.snapshot.reserve_balance))
    }

    fn palw_finalized_material_recovery_state_v2(
        &self,
    ) -> Result<Option<(BlockHash, kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1)>, String> {
        if self.config.params.palw_material_release_recovery_v2.is_none() {
            return Ok(None);
        }
        let before = self.lkg_virtual_state.load();
        let (generation, stable) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || generation.active.is_some() {
            return Err("terminal receipt import is incomplete".into());
        }
        let finality = self.virtual_processor.virtual_finality_point(&before.ghostdag_data, self.pruning_point());
        // Genesis commits no finalized parent state. Reading ORIGIN here would
        // turn a normal startup boundary into an authority read failure; None
        // still authorizes neither GC nor ACK/enrollment retirement.
        if finality == self.config.params.genesis.hash {
            return Ok(None);
        }
        if !self.services.reachability_service.has_reachability_data(finality)
            || !self.services.reachability_service.is_chain_ancestor_of(finality, before.ghostdag_data.selected_parent)
        {
            return Err("terminal receipt finalized anchor is unavailable".into());
        }
        // This is the state AFTER the selected parent committed by finality's
        // header, and therefore finalized whenever finality is finalized. It can
        // be reconstructed near the present pruning point without old Release.
        let parent = self.ghostdag_store.get_data(finality).map_err(|e| e.to_string())?.selected_parent;
        let state = self
            .virtual_processor
            .palw_candidate_state_v2_checked(parent)
            .map_err(|e| format!("terminal receipt finalized parent state unavailable: {e:?}"))?;
        let header = self.headers_store.get_header(finality).map_err(|e| e.to_string())?;
        kaspa_consensus_core::palw_state_proof_v1::state_root_of_pinned_header_v1(&header, finality).map_err(|e| e.to_string())?;
        if self.virtual_processor.palw_committed_root_v1(&state) != header.palw_state_root {
            return Err("terminal receipt reconstructed finalized state disagrees with header".into());
        }
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.config.params.palw_consensus_mode
        else {
            return Err("terminal receipt requires ConsensusV2".into());
        };
        state.assert_material_release_recovery_consistency_v2(&bundle.state).map_err(|e| e.to_string())?;
        let after = self.lkg_virtual_state.load();
        let (generation_after, stable_after) = self.virtual_processor.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable_after
            || generation_after.active.is_some()
            || generation.epoch != generation_after.epoch
            || before.ghostdag_data.selected_parent != after.ghostdag_data.selected_parent
        {
            return Err("terminal receipt changed finalized snapshot/import generation".into());
        }
        Ok(Some((finality, state)))
    }
}

#[cfg(test)]
mod evm_reader_fault_tests;

#[cfg(test)]
mod native_header_fault_tests;

#[cfg(test)]
mod canonicality_fault_tests;

#[cfg(all(test, feature = "evm"))]
mod evm_state_fault_tests;

#[cfg(test)]
mod evm_receipt_fault_tests;

#[cfg(test)]
mod evm_transaction_fault_tests;

#[cfg(test)]
mod evm_logs_fault_tests;

#[cfg(test)]
mod evm_parent_fault_tests;
