use crate::{
    consensus::{
        services::{
            ConsensusServices, DbBlockDepthManager, DbDagTraversalManager, DbGhostdagManager, DbParentsManager, DbPruningPointManager,
            DbWindowManager,
        },
        storage::ConsensusStorage,
    },
    errors::{BlockProcessResult, RuleError},
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            DB,
            block_window_cache::{BlockWindowCacheStore, BlockWindowCacheWriter, BlockWindowHeap},
            daa::DbDaaStore,
            depth::DbDepthStore,
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            headers_selected_tip::{DbHeadersSelectedTipStore, HeadersSelectedTipStoreReader},
            palw_lane_bits::{DbPalwLaneBitsStore, palw_lane_bits_child},
            palw_nullifier::{DbPalwNullifierStore, PalwNullifierStoreReader},
            palw_spam::{DbPalwSpamAccumulatorStore, PalwSpamAccumulatorV1},
            pruning::{DbPruningStore, PruningStoreReader},
            reachability::{DbReachabilityStore, StagingReachabilityStore},
            relations::{DbRelationsStore, RelationsStoreReader},
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
        },
    },
    params::Params,
    pipeline::deps_manager::{BlockProcessingMessage, BlockTask, BlockTaskDependencyManager, TaskId},
    processes::{ghostdag::ordering::SortableBlock, reachability::inquirer as reachability, relations::RelationsStoreExtensions},
};
use crossbeam_channel::{Receiver, Sender};
use itertools::Itertools;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHashSet, BlockLevel,
    blockhash::{BlockHashes, ORIGIN},
    blockstatus::BlockStatus::{self, StatusHeaderOnly, StatusInvalid},
    config::genesis::GenesisBlock,
    header::Header,
};
use kaspa_consensusmanager::SessionLock;
use kaspa_core::error;
use kaspa_database::prelude::{StoreResultExt, StoreResultUnitExt};
use kaspa_utils::vec::VecExtensions;
use parking_lot::RwLock;
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use std::sync::{Arc, atomic::Ordering};

use super::super::ProcessingCounters;

/// Batch-backed header stores publish cache entries while staging, before RocksDB commits. In
/// particular, unwinding a Rayon header job after staging the Header-v4 anti-spam row would let a
/// descendant observe state which a restart cannot recover. Once that row is staged, every failure
/// is therefore process-fatal and recovery starts from the last durable batch boundary.
#[cold]
#[inline(never)]
fn header_batch_commit_fail_stop(message: String) -> ! {
    error!("{message}");
    std::process::abort()
}

pub struct HeaderProcessingContext {
    pub hash: BlockHash,
    pub header: Arc<Header>,
    pub pruning_point: BlockHash,
    pub block_level: BlockLevel,
    pub known_direct_parents: BlockHashes,

    // Staging data
    pub ghostdag_data: Option<Arc<GhostdagData>>,
    pub block_window_for_difficulty: Option<Arc<BlockWindowHeap>>,
    pub block_window_for_past_median_time: Option<Arc<BlockWindowHeap>>,
    pub mergeset_non_daa: Option<BlockHashSet>,
    pub merge_depth_root: Option<BlockHash>,
    pub finality_point: Option<BlockHash>,
    pub palw_spam_state: Option<Arc<PalwSpamAccumulatorV1>>,
}

impl HeaderProcessingContext {
    pub fn new(
        hash: BlockHash,
        header: Arc<Header>,
        block_level: BlockLevel,
        pruning_point: BlockHash,
        known_direct_parents: BlockHashes,
    ) -> Self {
        Self {
            hash,
            header,
            block_level,
            pruning_point,
            known_direct_parents,
            ghostdag_data: None,
            block_window_for_difficulty: None,
            mergeset_non_daa: None,
            block_window_for_past_median_time: None,
            merge_depth_root: None,
            finality_point: None,
            palw_spam_state: None,
        }
    }

    /// Returns the primary (level 0) GHOSTDAG data of this header.
    /// NOTE: expected to be called only after GHOSTDAG computation was pushed into the context
    pub fn ghostdag_data(&self) -> &Arc<GhostdagData> {
        self.ghostdag_data.as_ref().unwrap()
    }
}

#[derive(Clone, Copy, Debug)]
struct HeaderCommitWriteStats {
    total_ops: u64,
    reachability_ops: u64,
    reachability_data_writes: u64,
}

pub struct HeaderProcessor {
    // Channels
    receiver: Receiver<BlockProcessingMessage>,
    body_sender: Sender<BlockProcessingMessage>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // Config
    pub(super) genesis: GenesisBlock,
    pub(super) timestamp_deviation_tolerance: u64,
    pub(super) max_block_parents: u8,
    pub(super) mergeset_size_limit: u64,
    pub(super) skip_proof_of_work: bool,
    pub(super) max_block_level: BlockLevel,
    /// PR-8.6: per-network domain-separation tag (`NetworkId::to_string`
    /// bytes) fed to the kaspa-pq Layer 0 PoW finalizer (ADR-0007 §4.2)
    /// during header PoW validation.
    pub(super) network_id: Vec<u8>,
    /// kaspa-pq Phase 3 PoW (ADR-0007): BLAKE2b-512 ∥ SHA3-512 (`algo_id = 3`) activation. Drives the
    /// per-header `pow_algo_id` rule (see `check_pow_algo_id`).
    pub(super) pow_blake2b_sha3_activation: kaspa_consensus_core::config::params::ForkActivation,
    /// kaspa-pq EVM Lane v0.4 (ADR-0020): drives the per-header version rule
    /// (see `check_header_version`) — v2 (`EVM_HEADER_VERSION`) required at and
    /// after activation, v1 (`BLOCK_VERSION`) before. `u64::MAX` (inert) on
    /// every current network.
    pub(super) evm_activation_daa_score: u64,
    /// ADR-0039 PALW DAA score at or after which Header-v3 and the algo-4 lane are active.
    pub(super) palw_activation_daa_score: u64,
    /// kaspa-pq ADR-0040 P0-3: the algo-4 ACCEPTANCE lever. While `false`, algo-4 headers are rejected in
    /// `check_pow_algo_id` — before GHOSTDAG, reachability, and every header-stage store write.
    pub(super) palw_algo4_accept: bool,
    /// kaspa-pq ADR-0039 §16.3 / C6 clause 7: the per-lane difficulty params. Read only in the gated
    /// lane-aware branch of `check_difficulty_and_daa_score` (`daa >= palw_activation`), so unused +
    /// byte-identical while PALW is inert.
    pub(super) palw_lane_difficulty: kaspa_consensus_core::palw::LaneDifficultyParams,
    /// kaspa-pq ADR-0039 PALW (§15.2): the active-nullifier retention window (DAA). Read in
    /// `commit_header` when writing the per-block set; unused while PALW is inert.
    pub(super) palw_nullifier_retention_daa: u64,
    /// Header-v4 re-genesis-only objective stamp and exact event-horizon accumulator parameters.
    pub(super) palw_spam: kaspa_consensus_core::palw_antispam::PalwSpamParams,
    /// ADR-MA: the Header-v5 / Compute Set registry activation fence (`u64::MAX` on every shipped
    /// preset — v5 unreachable, all new paths byte-identically inert).
    pub(super) palw_compute_registry_activation_daa_score: u64,

    // DB
    db: Arc<DB>,

    // Stores
    pub(super) relations_store: Arc<RwLock<DbRelationsStore>>,
    pub(super) reachability_store: Arc<RwLock<DbReachabilityStore>>,
    pub(super) reachability_relations_store: Arc<RwLock<DbRelationsStore>>,
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) pruning_point_store: Arc<RwLock<DbPruningStore>>,
    pub(super) block_window_cache_for_difficulty: Arc<BlockWindowCacheStore>,
    pub(super) block_window_cache_for_past_median_time: Arc<BlockWindowCacheStore>,
    pub(super) daa_excluded_store: Arc<DbDaaStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    /// Block-keyed two-lane difficulty frontier, advanced atomically with each active header.
    pub(super) palw_lane_bits_store: Arc<DbPalwLaneBitsStore>,
    /// kaspa-pq ADR-0039 PALW (§15.2): the per-block active-nullifier window store. Empty on every
    /// shipped preset (PALW inert); written in `commit_header` only when PALW is active.
    pub(super) palw_nullifier_store: Arc<DbPalwNullifierStore>,
    pub(super) palw_spam_store: Arc<DbPalwSpamAccumulatorStore>,
    pub(super) headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    pub(super) depth_store: Arc<DbDepthStore>,

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) _dag_traversal_manager: DbDagTraversalManager,
    pub(super) window_manager: DbWindowManager,
    pub(super) depth_manager: DbBlockDepthManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) _pruning_point_manager: DbPruningPointManager,
    pub(super) parents_manager: DbParentsManager,

    // Pruning lock
    pruning_lock: SessionLock,

    // Dependency manager
    task_manager: BlockTaskDependencyManager,

    // Counters
    counters: Arc<ProcessingCounters>,
}

impl HeaderProcessor {
    pub fn new(
        receiver: Receiver<BlockProcessingMessage>,
        body_sender: Sender<BlockProcessingMessage>,
        thread_pool: Arc<ThreadPool>,
        params: &Params,
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        services: &Arc<ConsensusServices>,
        pruning_lock: SessionLock,
        counters: Arc<ProcessingCounters>,
    ) -> Self {
        assert!(
            params.palw_spam.is_inert()
                || (params.palw_spam.is_structurally_valid()
                    && params.palw_activation_daa_score <= params.genesis.daa_score
                    && params.genesis.version == kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION),
            "non-inert PALW anti-spam parameters require a structurally valid Header-v4 re-genesis"
        );
        Self {
            receiver,
            body_sender,
            thread_pool,
            genesis: params.genesis.clone(),
            db,

            relations_store: storage.relations_store.clone(),
            reachability_store: storage.reachability_store.clone(),
            reachability_relations_store: storage.reachability_relations_store.clone(),
            ghostdag_store: storage.ghostdag_store.clone(),
            statuses_store: storage.statuses_store.clone(),
            pruning_point_store: storage.pruning_point_store.clone(),
            daa_excluded_store: storage.daa_excluded_store.clone(),
            headers_store: storage.headers_store.clone(),
            palw_lane_bits_store: storage.palw_lane_bits_store.clone(),
            palw_nullifier_store: storage.palw_nullifier_store.clone(),
            palw_spam_store: storage.palw_spam_store.clone(),
            depth_store: storage.depth_store.clone(),
            headers_selected_tip_store: storage.headers_selected_tip_store.clone(),
            block_window_cache_for_difficulty: storage.block_window_cache_for_difficulty.clone(),
            block_window_cache_for_past_median_time: storage.block_window_cache_for_past_median_time.clone(),

            ghostdag_manager: services.ghostdag_manager.clone(),
            _dag_traversal_manager: services.dag_traversal_manager.clone(),
            window_manager: services.window_manager.clone(),
            reachability_service: services.reachability_service.clone(),
            depth_manager: services.depth_manager.clone(),
            _pruning_point_manager: services.pruning_point_manager.clone(),
            parents_manager: services.parents_manager.clone(),

            task_manager: BlockTaskDependencyManager::new(),
            pruning_lock,
            counters,

            timestamp_deviation_tolerance: params.timestamp_deviation_tolerance,
            max_block_parents: params.max_block_parents(),
            mergeset_size_limit: params.mergeset_size_limit(),
            skip_proof_of_work: params.skip_proof_of_work,
            max_block_level: params.max_block_level,
            // PR-8.6: Layer 0 PoW per-network domain separation tag.
            network_id: params.net.to_string().into_bytes(),
            pow_blake2b_sha3_activation: params.pow_blake2b_sha3_activation,
            evm_activation_daa_score: params.evm_activation_daa_score,
            palw_activation_daa_score: params.palw_activation_daa_score,
            palw_algo4_accept: params.palw_algo4_accept,
            palw_lane_difficulty: params.palw_lane_difficulty.clone(),
            palw_nullifier_retention_daa: params.palw_nullifier_retention_daa,
            palw_spam: params.palw_spam,
            palw_compute_registry_activation_daa_score: params.palw_compute_registry_activation_daa_score,
        }
    }

    pub fn worker(self: &Arc<HeaderProcessor>) {
        while let Ok(msg) = self.receiver.recv() {
            match msg {
                BlockProcessingMessage::Exit => {
                    break;
                }
                BlockProcessingMessage::Process(task, block_result_transmitter, virtual_state_result_transmitter) => {
                    if let Some(task_id) = self.task_manager.register(task, block_result_transmitter, virtual_state_result_transmitter)
                    {
                        let processor = self.clone();
                        self.thread_pool.spawn(move || {
                            processor.queue_block(task_id);
                        });
                    }
                }
            };
        }

        // Wait until all workers are idle before exiting
        self.task_manager.wait_for_idle();

        // Pass the exit signal on to the following processor
        self.body_sender.send(BlockProcessingMessage::Exit).unwrap();
    }

    fn queue_block(self: &Arc<HeaderProcessor>, task_id: TaskId) {
        if let Some(task) = self.task_manager.try_begin(task_id) {
            let res = self.process_header(&task);

            let dependent_tasks = self.task_manager.end(
                task,
                |task,
                 block_result_transmitter: tokio::sync::oneshot::Sender<Result<BlockStatus, RuleError>>,
                 virtual_state_result_transmitter| {
                    if res.is_err() || task.block().is_header_only() {
                        // We don't care if receivers were dropped
                        let _ = block_result_transmitter.send(res.clone());
                        let _ = virtual_state_result_transmitter.send(res.clone());
                    } else {
                        self.body_sender
                            .send(BlockProcessingMessage::Process(task, block_result_transmitter, virtual_state_result_transmitter))
                            .unwrap();
                    }
                },
            );

            for dep in dependent_tasks {
                let processor = self.clone();
                self.thread_pool.spawn(move || processor.queue_block(dep));
            }
        }
    }

    fn process_header(&self, task: &BlockTask) -> BlockProcessResult<BlockStatus> {
        let _prune_guard = self.pruning_lock.blocking_read();
        let header = &task.block().header;
        let status_option = self.statuses_store.read().get(header.hash).optional().unwrap();

        match status_option {
            Some(StatusInvalid) => return Err(RuleError::KnownInvalid),
            Some(status) => return Ok(status),
            None => {}
        }

        // Validate the header depending on task type
        match task {
            BlockTask::Ordinary { .. } => {
                // [ibd-perf §7-1] split-time Phase-A (parallelizable compute) vs the serial committer.
                let hp_t0 = std::time::Instant::now();
                let ctx = self.validate_header(header)?;
                let hp_t1 = std::time::Instant::now();
                let hp_write_stats = self.commit_header(ctx, header);
                let hp_t2 = std::time::Instant::now();
                self.counters.hdr_validate_ns.fetch_add((hp_t1 - hp_t0).as_nanos() as u64, Ordering::Relaxed);
                self.counters.hdr_commit_ns.fetch_add((hp_t2 - hp_t1).as_nanos() as u64, Ordering::Relaxed);
                self.counters.hdr_dbwrite_batches.fetch_add(1, Ordering::Relaxed);
                self.counters.hdr_dbwrite_ops.fetch_add(hp_write_stats.total_ops, Ordering::Relaxed);
                self.counters.hdr_reachability_dbwrite_ops.fetch_add(hp_write_stats.reachability_ops, Ordering::Relaxed);
                self.counters.hdr_reachability_data_writes.fetch_add(hp_write_stats.reachability_data_writes, Ordering::Relaxed);
                self.counters.hdr_timed_counts.fetch_add(1, Ordering::Relaxed);
            }
            BlockTask::Trusted { .. } => {
                let ctx = self.validate_trusted_header(header)?;
                self.commit_trusted_header(ctx, header);
            }
        }

        // Report counters
        self.counters.header_counts.fetch_add(1, Ordering::Relaxed);
        self.counters.dep_counts.fetch_add(header.direct_parents().len() as u64, Ordering::Relaxed);

        Ok(StatusHeaderOnly)
    }

    /// Runs full ordinary header validation
    fn validate_header(&self, header: &Arc<Header>) -> BlockProcessResult<HeaderProcessingContext> {
        let block_level = self.validate_header_in_isolation(header)?;
        self.validate_parent_relations(header)?;
        let mut ctx = self.build_processing_context(header, block_level);
        self.ghostdag(&mut ctx);
        self.pre_pow_validation(&mut ctx, header)?;
        if let Err(e) = self.post_pow_validation(&mut ctx, header) {
            self.statuses_store.write().set(ctx.hash, StatusInvalid).unwrap();
            return Err(e);
        }
        Ok(ctx)
    }

    // Runs partial header validation for trusted blocks (currently validates only header-in-isolation and computes GHOSTDAG).
    fn validate_trusted_header(&self, header: &Arc<Header>) -> BlockProcessResult<HeaderProcessingContext> {
        let block_level = self.validate_header_in_isolation(header)?;
        let mut ctx = self.build_processing_context(header, block_level);
        self.ghostdag(&mut ctx);
        if !self.palw_spam.is_inert() {
            self.check_mergeset_size_limit(&mut ctx)?;
            self.check_palw_spam(&mut ctx, header)?;
        }
        Ok(ctx)
    }

    fn build_processing_context(&self, header: &Arc<Header>, block_level: u8) -> HeaderProcessingContext {
        HeaderProcessingContext::new(
            header.hash,
            header.clone(),
            block_level,
            self.pruning_point_store.read().pruning_point().unwrap(),
            self.collect_known_direct_parents(header),
        )
    }

    fn collect_known_direct_parents(&self, header: &Header) -> BlockHashes {
        let relations_read = self.relations_store.read();
        Arc::new(
            header
                .direct_parents()
                .iter()
                .copied()
                // filter out parents not part of the kept contiguous Dag - which is representd by the stored relations 
                .filter(|&parent| relations_read.has(parent).unwrap())
                .collect_vec()
                // This kicks-in only for trusted blocks. If an ordinary block is 
                // missing direct parents it will fail validation.
                .push_if_empty(ORIGIN),
        )
    }

    /// Runs the GHOSTDAG algorithm and writes the data into the context (if hasn't run already)
    fn ghostdag(&self, ctx: &mut HeaderProcessingContext) {
        let ghostdag_data = self
            .ghostdag_store
            .get_data(ctx.hash)
            .optional()
            .unwrap()
            .unwrap_or_else(|| Arc::new(self.ghostdag_manager.ghostdag(&ctx.known_direct_parents)));
        self.counters.mergeset_counts.fetch_add(ghostdag_data.mergeset_size() as u64, Ordering::Relaxed);
        ctx.ghostdag_data = Some(ghostdag_data);
    }

    /// Advance the block-keyed two-lane frontier in the same batch as `header`. Ordinary processing
    /// fails closed if an active selected-parent row is absent. Trusted pruning-proof headers may
    /// precede installation of the pruning-point sidecar, so that path skips only while its parent
    /// frontier is unavailable; trusted blocks above the imported boundary then advance normally.
    fn stage_palw_lane_bits(
        &self,
        batch: &mut WriteBatch,
        ctx: &HeaderProcessingContext,
        header: &Header,
        allow_unseeded_trusted_boundary: bool,
    ) {
        if header.daa_score < self.palw_activation_daa_score || ctx.hash == self.genesis.hash {
            return;
        }
        let selected_parent = ctx.ghostdag_data.as_ref().unwrap().selected_parent;
        let selected_parent_active = selected_parent != self.genesis.hash
            && self.headers_store.get_daa_score(selected_parent).unwrap() >= self.palw_activation_daa_score;
        let parent = if selected_parent_active {
            match self
                .palw_lane_bits_store
                .lane_bits(selected_parent)
                .unwrap_or_else(|err| panic!("failed reading PALW lane frontier for selected parent {selected_parent}: {err}"))
            {
                Some(bits) => Some(bits),
                None if allow_unseeded_trusted_boundary => return,
                None => panic!("missing PALW lane frontier for active selected parent {selected_parent}"),
            }
        } else {
            None
        };
        let next = palw_lane_bits_child(
            parent,
            self.palw_lane_difficulty.genesis_hash_bits,
            self.palw_lane_difficulty.genesis_replica_bits,
            header.pow_algo_id,
            header.bits,
        )
        .unwrap_or_else(|err| panic!("cannot advance PALW lane frontier for {}: {err}", ctx.hash));
        self.palw_lane_bits_store
            .set_batch(batch, ctx.hash, next)
            .unwrap_or_else(|err| panic!("failed staging PALW lane frontier for {}: {err}", ctx.hash));
    }

    fn commit_header(&self, ctx: HeaderProcessingContext, header: &Header) -> HeaderCommitWriteStats {
        let ghostdag_data = ctx.ghostdag_data.as_ref().unwrap();

        // Create a DB batch writer
        let mut batch = WriteBatch::default();

        //
        // Append-only stores: these require no lock and hence done first in order to reduce locking time
        //
        self.ghostdag_store.insert_batch(&mut batch, ctx.hash, ghostdag_data).unwrap();
        self.stage_palw_lane_bits(&mut batch, &ctx, header, false);

        // kaspa-pq ADR-0039 PALW (§15.2): persist this block's active-nullifier window so descendants
        // seed their duplicate-ticket dedup from it without re-walking history. The set = the selected
        // parent's window ∪ this block's UNIQUE algo-4 mergeset ticket nullifiers (duplicates were
        // already colored red by GHOSTDAG, so the blue set is unique), pruned to the retention window.
        // Mainnet, testnet-10, simnet and devnet keep this store empty with an activation score of
        // `u64::MAX`; the three PALW presets write a row for every non-genesis block.
        //
        // GENESIS boundary (the re-genesis root, when `palw_activation_daa_score <= genesis.daa_score`):
        // genesis is the parentless trusted root — its GHOSTDAG selected parent is `blockhash::ORIGIN`, not
        // a stored block, so the `get_daa_score(sp)` below would panic. It has no ancestor window to inherit
        // (the first PALW child seeds empty via `sp == genesis.hash`), so skip the fold and write no window.
        // Mirrors the genesis guard in `commit_palw_beacon_state`.
        if header.daa_score >= self.palw_activation_daa_score && ctx.hash != self.genesis.hash {
            let sp = ghostdag_data.selected_parent;
            // FAIL-CLOSED (matches the beacon accumulator + the GHOSTDAG seed): an active, non-genesis
            // selected parent MUST have persisted its window here, so a store miss is a consensus-state
            // invariant break to halt on — NOT the old `unwrap_or_default()`, which silently dropped every
            // inherited ancestor nullifier and re-opened cross-ancestor ticket reuse (fail-OPEN). Boundary-
            // aware: an SP predating activation (or the re-genesis block itself) legitimately has no window,
            // so it seeds empty.
            let sp_active = sp != self.genesis.hash && self.headers_store.get_daa_score(sp).unwrap() >= self.palw_activation_daa_score;
            let mut set: kaspa_consensus_core::palw::PalwActiveNullifierSet = if sp_active {
                (*self
                    .palw_nullifier_store
                    .get(sp)
                    .unwrap_or_else(|err| panic!("missing PALW nullifier window for active selected parent {sp}: {err}")))
                .clone()
            } else {
                kaspa_consensus_core::palw::PalwActiveNullifierSet::default()
            };
            for &blue in ghostdag_data.mergeset_blues.iter() {
                let h = self.headers_store.get_header(blue).unwrap();
                if h.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA {
                    set.insert(h.palw_ticket_nullifier, h.daa_score);
                }
            }
            // Retention prune to the network's PALW nullifier-retention window (§15.2).
            set.prune_below(header.daa_score.saturating_sub(self.palw_nullifier_retention_daa));
            self.palw_nullifier_store.insert_batch(&mut batch, ctx.hash, Arc::new(set)).unwrap();
        }

        if let Some(window) = ctx.block_window_for_difficulty {
            self.block_window_cache_for_difficulty.insert(ctx.hash, window);
        }
        if let Some(window) = ctx.block_window_for_past_median_time {
            self.block_window_cache_for_past_median_time.insert(ctx.hash, window);
        }

        self.daa_excluded_store.insert_batch(&mut batch, ctx.hash, Arc::new(ctx.mergeset_non_daa.unwrap())).unwrap();
        self.headers_store.insert_batch(&mut batch, ctx.hash, ctx.header, ctx.block_level).unwrap();
        self.depth_store.insert_batch(&mut batch, ctx.hash, ctx.merge_depth_root.unwrap(), ctx.finality_point.unwrap()).unwrap();

        //
        // Reachability and header chain stores
        //

        // Create staging reachability store. We use an upgradable read here to avoid concurrent
        // staging reachability operations. PERF: we assume that reachability processing time << header processing
        // time, and thus serializing this part will do no harm. However this should be benchmarked. The
        // alternative is to create a separate ReachabilityProcessor and to manage things more tightly.
        // [ibd-perf §7-1] held-lock window starts at the upgradable_read acquisition.
        let hp_lock_t0 = std::time::Instant::now();
        let mut staging = StagingReachabilityStore::new(self.reachability_store.upgradable_read());
        let selected_parent = ghostdag_data.selected_parent;
        let mut reachability_mergeset = ghostdag_data.unordered_mergeset_without_selected_parent();
        let hp_add_t0 = std::time::Instant::now();
        reachability::add_block(&mut staging, ctx.hash, selected_parent, &mut reachability_mergeset).unwrap();
        self.counters.hdr_addblock_ns.fetch_add(hp_add_t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

        // Non-append only stores need to use write locks.
        // Note we need to keep the lock write guards until the batch is written.
        let mut hst_write = self.headers_selected_tip_store.write();
        let prev_hst = hst_write.get().unwrap();
        if SortableBlock::new(ctx.hash, header.blue_work) > prev_hst
            && reachability::is_chain_ancestor_of(&staging, ctx.pruning_point, ctx.hash).unwrap()
        {
            // Hint reachability about the new tip.
            reachability::hint_virtual_selected_parent(&mut staging, ctx.hash).unwrap();
            hst_write.set_batch(&mut batch, SortableBlock::new(ctx.hash, header.blue_work)).unwrap();
        }

        //
        // Relations and statuses
        //

        let mut relations_write = self.relations_store.write();
        relations_write.insert_batch(&mut batch, ctx.hash, ctx.known_direct_parents.clone()).unwrap();

        // Write reachability relations. These relations are only needed during header pruning
        let mut reachability_relations_write = self.reachability_relations_store.write();
        reachability_relations_write.insert_batch(&mut batch, ctx.hash, ctx.known_direct_parents).unwrap();

        let statuses_write = self.statuses_store.set_batch(&mut batch, ctx.hash, StatusHeaderOnly).unwrap();

        // Write reachability data. Only at this brief moment the reachability store is locked for reads.
        // We take special care for this since reachability read queries are used throughout the system frequently.
        // Note we hold the lock until the batch is written
        let hp_reachability_data_writes = staging.staged_data_write_count() as u64;
        let hp_reachability_ops_before = batch.len() as u64;
        let reachability_write = staging.commit(&mut batch).unwrap();
        let hp_reachability_ops = batch.len() as u64 - hp_reachability_ops_before;

        // Stage the cache-backed anti-spam row last: no recoverable/fallible work may follow except
        // the final RocksDB commit, whose failure is fail-stop for cache/disk equivalence.
        if let Some(state) = ctx.palw_spam_state.as_ref()
            && let Err(err) = self.palw_spam_store.insert_batch(&mut batch, ctx.hash, state.clone())
        {
            header_batch_commit_fail_stop(format!("failed staging PALW spam accumulator for header {}: {err}", ctx.hash));
        }

        // Flush the batch to the DB.
        let hp_write_ops = batch.len() as u64;
        let hp_write_t0 = std::time::Instant::now();
        if let Err(err) = self.db.write(batch) {
            header_batch_commit_fail_stop(format!("atomic header batch write failed for {}: {err}", ctx.hash));
        }
        self.counters.hdr_dbwrite_ns.fetch_add(hp_write_t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
        self.counters.hdr_heldlock_ns.fetch_add(hp_lock_t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(reachability_write);
        drop(statuses_write);
        drop(reachability_relations_write);
        drop(relations_write);
        drop(hst_write);
        HeaderCommitWriteStats {
            total_ops: hp_write_ops,
            reachability_ops: hp_reachability_ops,
            reachability_data_writes: hp_reachability_data_writes,
        }
    }

    fn commit_trusted_header(&self, ctx: HeaderProcessingContext, header: &Header) {
        let ghostdag_data = ctx.ghostdag_data.as_ref().unwrap();

        // Create a DB batch writer
        let mut batch = WriteBatch::default();

        // This data might have been already written when applying the pruning proof.
        self.ghostdag_store.insert_batch(&mut batch, ctx.hash, ghostdag_data).idempotent().unwrap();
        self.stage_palw_lane_bits(&mut batch, &ctx, header, true);

        let mut relations_write = self.relations_store.write();
        relations_write.insert_batch(&mut batch, ctx.hash, ctx.known_direct_parents).idempotent().unwrap();

        let statuses_write = self.statuses_store.set_batch(&mut batch, ctx.hash, StatusHeaderOnly).unwrap();

        // As in ordinary header processing, stage the anti-spam cache last and never unwind after it.
        if let Some(state) = ctx.palw_spam_state.as_ref()
            && let Err(err) = self.palw_spam_store.insert_batch(&mut batch, ctx.hash, state.clone()).idempotent()
        {
            header_batch_commit_fail_stop(format!("failed staging trusted PALW spam accumulator for header {}: {err}", ctx.hash));
        }

        // Flush the batch to the DB.
        if let Err(err) = self.db.write(batch) {
            header_batch_commit_fail_stop(format!("atomic trusted-header batch write failed for {}: {err}", ctx.hash));
        }

        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(statuses_write);
        drop(relations_write);
    }

    pub fn process_genesis(&self) {
        // Init headers selected tip and selected chain stores
        let mut batch = WriteBatch::default();
        let mut hst_write = self.headers_selected_tip_store.write();
        hst_write.set_batch(&mut batch, SortableBlock::new(self.genesis.hash, 0.into())).unwrap();
        self.db.write(batch).unwrap();
        drop(hst_write);

        // Write the genesis header
        let mut genesis_header: Header = (&self.genesis).into();
        // Force the provided genesis hash. Important for some tests which manually modify the genesis hash.
        // Note that for official nets (mainnet, testnet etc) they are guaranteed to be equal as enforced by a test in genesis.rs
        genesis_header.hash = self.genesis.hash;
        let genesis_header = Arc::new(genesis_header);

        let mut ctx = HeaderProcessingContext::new(
            self.genesis.hash,
            genesis_header.clone(),
            self.max_block_level,
            self.genesis.hash,
            BlockHashes::new(vec![ORIGIN]),
        );
        ctx.ghostdag_data = Some(Arc::new(self.ghostdag_manager.genesis_ghostdag_data()));
        ctx.mergeset_non_daa = Some(Default::default());
        ctx.merge_depth_root = Some(ORIGIN);
        ctx.finality_point = Some(ORIGIN);
        if !self.palw_spam.is_inert() {
            let state = PalwSpamAccumulatorV1::root(genesis_header.daa_score);
            assert_eq!(
                genesis_header.palw_spam_accumulator_commitment,
                state.commitment(),
                "Header-v4 re-genesis must commit its root PALW anti-spam accumulator"
            );
            ctx.palw_spam_state = Some(Arc::new(state));
        }

        self.commit_header(ctx, &genesis_header);
    }

    pub fn init(&self) {
        if self.relations_store.read().has(ORIGIN).unwrap() {
            return;
        }

        let mut batch = WriteBatch::default();
        let mut relations_write = self.relations_store.write();
        relations_write.insert_batch(&mut batch, ORIGIN, BlockHashes::new(vec![])).unwrap();
        self.ghostdag_store.insert_batch(&mut batch, ORIGIN, &self.ghostdag_manager.origin_ghostdag_data()).unwrap();
        let mut hst_write = self.headers_selected_tip_store.write();
        hst_write.set_batch(&mut batch, SortableBlock::new(ORIGIN, 0.into())).unwrap();
        self.db.write(batch).unwrap();
        drop(hst_write);
        drop(relations_write);
    }
}
