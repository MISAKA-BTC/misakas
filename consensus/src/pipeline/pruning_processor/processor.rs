//! TODO: module comment about locking safety and consistency of various pruning stores

use crate::{
    consensus::{
        services::{ConsensusServices, DbParentsManager, DbPruningPointManager},
        storage::ConsensusStorage,
    },
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            evm::{EvmHeaderStore, EvmPayloadStore, EvmStateStore},
            ghostdag::{CompactGhostdagData, GhostdagStoreReader},
            headers::HeaderStoreReader,
            palw_spam::palw_spam_reclaim_candidates,
            past_pruning_points::PastPruningPointsStoreReader,
            pruning::PruningStoreReader,
            pruning_overlay_snapshot::PruningPointOverlaySnapshotStoreReader,
            pruning_samples::PruningSamplesStoreReader,
            reachability::{DbReachabilityStore, ReachabilityStoreReader, StagingReachabilityStore},
            relations::StagingRelationsStore,
            selected_chain::{SelectedChainStore, SelectedChainStoreReader},
            statuses::StatusesStoreReader,
            tips::{TipsStore, TipsStoreReader},
            utxo_diffs::UtxoDiffsStoreReader,
            virtual_state::VirtualStateStoreReader,
        },
    },
    pipeline::virtual_processor::VirtualStateProcessor,
    processes::{pruning_proof::PruningProofManager, reachability::inquirer as reachability, relations},
};
use crossbeam_channel::Receiver as CrossbeamReceiver;
use itertools::Itertools;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, BlockLevel,
    blockhash::ORIGIN,
    blockstatus::BlockStatus::StatusHeaderOnly,
    config::Config,
    muhash::MuHashExtensions,
    pruning::{PruningPointProof, PruningPointTrustedData},
    trusted::ExternalGhostdagData,
};
use kaspa_consensusmanager::SessionLock;
use kaspa_core::{debug, error, info, trace, warn};
use kaspa_database::prelude::{BatchDbWriter, DB, MemoryWriter, StoreResultExt};
use kaspa_muhash::MuHash;
use kaspa_utils::iter::IterExtensions;
use parking_lot::RwLockUpgradableReadGuard;
use rocksdb::WriteBatch;
use std::{
    collections::{VecDeque, hash_map::Entry::Vacant},
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub enum PruningProcessingMessage {
    Exit,
    Process { sink_ghostdag_data: CompactGhostdagData },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PalwSnapshotRecoveryPlan {
    None,
    RebuildFromRetainedRows,
    FailClosed,
}

fn palw_snapshot_recovery_plan(non_genesis: bool, snapshot_valid: bool, source_rows_pruned: bool) -> PalwSnapshotRecoveryPlan {
    if !non_genesis || snapshot_valid {
        PalwSnapshotRecoveryPlan::None
    } else if source_rows_pruned {
        PalwSnapshotRecoveryPlan::FailClosed
    } else {
        PalwSnapshotRecoveryPlan::RebuildFromRetainedRows
    }
}

fn pruning_boundary_source_rows_pruned(is_archival: bool, retention_checkpoint: BlockHash, retention_period_root: BlockHash) -> bool {
    !is_archival && retention_checkpoint == retention_period_root
}

/// Batch-backed singleton caches advance before RocksDB commits. Once boundary staging starts, a
/// recoverable return would allow the next worker iteration to observe unpersisted sidecars and prune
/// their reconstruction rows. Abort the process and restart from the last atomic DB boundary instead.
#[cold]
#[inline(never)]
fn pruning_boundary_commit_fail_stop(message: String) -> ! {
    error!("{message}");
    std::process::abort()
}

/// A processor dedicated for moving the pruning point and pruning any possible data in its past
pub struct PruningProcessor {
    // Channels
    receiver: CrossbeamReceiver<PruningProcessingMessage>,

    // DB
    db: Arc<DB>,

    // Storage
    storage: Arc<ConsensusStorage>,

    // Managers and Services
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    pruning_point_manager: DbPruningPointManager,
    pruning_proof_manager: Arc<PruningProofManager>,
    parents_manager: DbParentsManager,

    // kaspa-pq ADR-0022: used to capture the as-of-pruning-point overlay snapshot via the
    // same compute path the virtual processor validates with (before below-pp rows are pruned).
    virtual_processor: Arc<VirtualStateProcessor>,

    // Pruning lock
    pruning_lock: SessionLock,

    // Config
    config: Arc<Config>,

    // Signals
    is_consensus_exiting: Arc<AtomicBool>,
}

impl Deref for PruningProcessor {
    type Target = ConsensusStorage;

    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl PruningProcessor {
    pub fn new(
        receiver: CrossbeamReceiver<PruningProcessingMessage>,
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        services: &Arc<ConsensusServices>,
        virtual_processor: Arc<VirtualStateProcessor>,
        pruning_lock: SessionLock,
        config: Arc<Config>,
        is_consensus_exiting: Arc<AtomicBool>,
    ) -> Self {
        Self {
            receiver,
            db,
            storage: storage.clone(),
            reachability_service: services.reachability_service.clone(),
            pruning_point_manager: services.pruning_point_manager.clone(),
            pruning_proof_manager: services.pruning_proof_manager.clone(),
            parents_manager: services.parents_manager.clone(),
            virtual_processor,
            pruning_lock,
            config,
            is_consensus_exiting,
        }
    }

    pub fn worker(self: &Arc<Self>) {
        // On start-up, check if any pruning workflows require recovery. We wait for the first processing message to arrive
        // in order to make sure the node is already connected and receiving blocks before we start background recovery operations
        let mut recovered = false;
        while let Ok(PruningProcessingMessage::Process { sink_ghostdag_data }) = self.receiver.recv() {
            if !recovered {
                if !self.recover_pruning_workflows_if_needed() {
                    // Recovery could fail for several reasons:
                    // (a) Consensus has exited while it was undergoing
                    // (b) Consensus is in a transitional state
                    // (c) Consensus is no longer in a transitional state per-se but has yet to catch up on sufficient block data
                    // For (a), the best course of measure is to exit the loop
                    // For (b)+(c), it is to attempt it again
                    // Continuing the loop satisfies both since if consensus exited the next iteration of the loop will exit as well
                    continue;
                }
                recovered = true;
            }
            self.advance_pruning_point_if_possible(sink_ghostdag_data);
        }
    }

    pub(crate) fn recover_pruning_workflows_if_needed(&self) -> bool {
        // returns true if recovery was completed successfully or was not needed
        // Serialize boundary inspection/rebuild with every guarded IBD session. This guard is dropped
        // before UTXO advancement and `prune`, the latter of which takes the same lock in write mode.
        let boundary_guard = self.pruning_lock.blocking_write();
        let pruning_point_read = self.pruning_point_store.read();
        let pruning_point = pruning_point_read.pruning_point().unwrap();
        let retention_checkpoint = pruning_point_read.retention_checkpoint().unwrap();
        let retention_period_root = pruning_point_read.retention_period_root().unwrap();
        let pruning_meta_read = self.pruning_meta_stores.read();
        let pruning_utxoset_position = pruning_meta_read.utxoset_position().unwrap();
        drop(pruning_point_read);
        drop(pruning_meta_read);

        // A new binary may encounter a pre-feature datadir whose pp pointer moved before all complete
        // boundary sidecars existed. Repair is allowed only while their source rows remain. Once
        // pruning completed, a missing/stale PALW or DNS overlay singleton is unrecoverable and must
        // stay fail-closed; otherwise the next prune would delete the only reconstruction source.
        let source_rows_pruned =
            pruning_boundary_source_rows_pruned(self.config.is_archival, retention_checkpoint, retention_period_root);
        let palw_plan = palw_snapshot_recovery_plan(
            pruning_point != self.config.genesis.hash,
            self.virtual_processor.pruning_point_palw_snapshot().is_some(),
            source_rows_pruned,
        );
        let overlay_required = pruning_point != self.config.genesis.hash && self.config.params.dns_params.is_some();
        let overlay_valid = !overlay_required
            || self.pruning_overlay_snapshot_store.read().get().is_ok_and(|snapshot| snapshot.pruning_point == pruning_point);
        let overlay_plan = palw_snapshot_recovery_plan(overlay_required, overlay_valid, source_rows_pruned);

        if matches!(palw_plan, PalwSnapshotRecoveryPlan::FailClosed) || matches!(overlay_plan, PalwSnapshotRecoveryPlan::FailClosed) {
            error!(
                "pruning boundary sidecar is missing/stale/corrupt for already-pruned point {}; restore a matching full datadir or snapshot",
                pruning_point
            );
            return false;
        }

        if matches!(palw_plan, PalwSnapshotRecoveryPlan::RebuildFromRetainedRows)
            || matches!(overlay_plan, PalwSnapshotRecoveryPlan::RebuildFromRetainedRows)
        {
            // Complete every semantic build before the first cache-backed `set_batch` call.
            let repaired_palw = if matches!(palw_plan, PalwSnapshotRecoveryPlan::RebuildFromRetainedRows) {
                match self.virtual_processor.build_palw_pruning_point_snapshot(pruning_point) {
                    Ok(snapshot) => Some(snapshot),
                    Err(err) => {
                        error!("cannot deterministically repair PALW pruning snapshot for {pruning_point}: {err}");
                        return false;
                    }
                }
            } else {
                None
            };
            let repaired_overlay = if matches!(overlay_plan, PalwSnapshotRecoveryPlan::RebuildFromRetainedRows) {
                let Some(snapshot) = self.virtual_processor.build_pruning_point_overlay_snapshot(pruning_point) else {
                    error!("DNS overlay snapshot is required but cannot be rebuilt for {pruning_point}");
                    return false;
                };
                Some(snapshot)
            } else {
                None
            };

            // Staging starts here; failures are no longer safely recoverable in-process because the
            // caches may be ahead of RocksDB.
            let mut batch = WriteBatch::default();
            if let Some(snapshot) = repaired_palw {
                if let Some(da) = snapshot.payload.da_snapshot.as_ref()
                    && let Err(err) = self.palw_da_store.write().set_pruning_snapshot_batch(&mut batch, da)
                {
                    pruning_boundary_commit_fail_stop(format!(
                        "failed to stage repaired PALW DA pruning snapshot for {pruning_point}: {err}"
                    ));
                }
                if let Some(search) = snapshot.payload.search_availability_snapshot.as_ref()
                    && let Err(err) = self.palw_search_availability_store.write().set_pruning_snapshot_batch(&mut batch, search)
                {
                    pruning_boundary_commit_fail_stop(format!(
                        "failed to stage repaired PALW search-availability pruning snapshot for {pruning_point}: {err}"
                    ));
                }
                self.palw_pruned_frontier_store.write().set_batch(&mut batch, snapshot).unwrap_or_else(|err| {
                    pruning_boundary_commit_fail_stop(format!(
                        "failed to stage repaired PALW pruning snapshot for {pruning_point}: {err}"
                    ))
                });
            }
            if let Some(snapshot) = repaired_overlay {
                self.pruning_overlay_snapshot_store.write().set_batch(&mut batch, snapshot).unwrap_or_else(|err| {
                    pruning_boundary_commit_fail_stop(format!(
                        "failed to stage repaired DNS overlay pruning snapshot for {pruning_point}: {err}"
                    ))
                });
            }
            if let Err(err) = self.db.write(batch) {
                pruning_boundary_commit_fail_stop(format!(
                    "failed to atomically persist repaired pruning sidecars for {pruning_point}: {err}"
                ));
            }
            info!("Repaired complete pruning boundary sidecars for {} before resuming prune", pruning_point);
        }

        drop(boundary_guard);

        debug!(
            "[PRUNING PROCESSOR] recovery check: current pruning point: {}, retention checkpoint: {:?}, pruning utxoset position: {:?}",
            pruning_point, retention_checkpoint, pruning_utxoset_position
        );

        // This indicates the node crashed during a former pruning point move and we need to recover
        if pruning_utxoset_position != pruning_point {
            info!("Recovering pruning utxo-set from {} to the pruning point {}", pruning_utxoset_position, pruning_point);
            if !self.advance_pruning_utxoset(pruning_utxoset_position, pruning_point) {
                info!("Interrupted while advancing the pruning point UTXO set: Process is exiting");
                return false;
            }
        }
        // The following two checks are implicitly checked in advance_pruning_utxoset, and hence can theoretically
        // be skipped if that function was called. As these checks are cheap, we  perform them regardless
        // as to not complicate the logic.

        // If the latest pruning point is the result of an IBD catchup, it is guaranteed that the headers selected tip
        // is pruning_depth on top of it
        // but crucially it is not guaranteed *virtual* is of sufficient depth above it
        // internally the pruning process checks this process for virtual and fails otherwise
        // for this reason, pruning is held until virtual has advanced enough.
        if !self.confirm_pruning_depth_below_virtual(pruning_point) {
            return false;
        }
        let pruning_meta_read = self.pruning_meta_stores.read();

        // don't prune if in a transitional ibd state.
        if pruning_meta_read.is_in_transitional_ibd_state() {
            return false;
        }

        drop(pruning_meta_read);
        trace!(
            "retention_checkpoint: {:?} | retention_period_root: {} | pruning_point: {}",
            retention_checkpoint, retention_period_root, pruning_point
        );

        // This indicates the node crashed or was forced to stop during a former data prune operation hence
        // we need to complete it
        if retention_checkpoint != retention_period_root {
            self.prune(pruning_point, retention_period_root);
        }
        true
    }

    fn advance_pruning_point_if_possible(&self, sink_ghostdag_data: CompactGhostdagData) {
        // Boundary capture and pointer/sidecar commit are one serialized unit. Drop before UTXO
        // advancement and `prune`, which reacquires this same lock in write mode.
        let boundary_guard = self.pruning_lock.blocking_write();
        let pruning_point_read = self.pruning_point_store.upgradable_read();
        let (current_pruning_point, current_index) = pruning_point_read.pruning_point_and_index().unwrap();
        let new_pruning_points = self.pruning_point_manager.next_pruning_points(sink_ghostdag_data, current_pruning_point);

        if let Some(new_pruning_point) = new_pruning_points.last().copied() {
            let retention_period_root = pruning_point_read.retention_period_root().unwrap();

            // Build while every below-pp source row still exists. The resulting value is committed in
            // the same batch as the pp pointer below, eliminating the crash state "new pp, stale PALW
            // snapshot". Failure is a pruning stop, never a partial downgrade to the legacy frontier.
            let palw_snapshot = match self.virtual_processor.build_palw_pruning_point_snapshot(new_pruning_point) {
                Ok(snapshot) => snapshot,
                Err(err) => {
                    error!("Refusing to advance pruning point to {new_pruning_point}: PALW frontier capture failed: {err}");
                    return;
                }
            };
            let overlay_snapshot = if self.config.params.dns_params.is_some() {
                let Some(snapshot) = self.virtual_processor.build_pruning_point_overlay_snapshot(new_pruning_point) else {
                    error!("Refusing to advance pruning point to {new_pruning_point}: required DNS overlay capture failed");
                    return;
                };
                Some(snapshot)
            } else {
                None
            };

            // Update past pruning points and pruning point stores
            let mut batch = WriteBatch::default();
            let mut pruning_point_write = RwLockUpgradableReadGuard::upgrade(pruning_point_read);
            for (i, past_pp) in new_pruning_points.iter().copied().enumerate() {
                self.past_pruning_points_store.insert_batch(&mut batch, current_index + i as u64 + 1, past_pp).unwrap_or_else(|err| {
                    pruning_boundary_commit_fail_stop(format!("failed staging periodic past pruning point: {err}"))
                });
            }
            let new_pp_index = current_index + new_pruning_points.len() as u64;
            pruning_point_write
                .set_batch(&mut batch, new_pruning_point, new_pp_index)
                .unwrap_or_else(|err| pruning_boundary_commit_fail_stop(format!("failed staging periodic pruning pointer: {err}")));
            if let Some(da) = palw_snapshot.payload.da_snapshot.as_ref() {
                self.palw_da_store.write().set_pruning_snapshot_batch(&mut batch, da).unwrap_or_else(|err| {
                    pruning_boundary_commit_fail_stop(format!("failed staging periodic PALW DA boundary: {err}"))
                });
            }
            if let Some(search) = palw_snapshot.payload.search_availability_snapshot.as_ref() {
                self.palw_search_availability_store.write().set_pruning_snapshot_batch(&mut batch, search).unwrap_or_else(|err| {
                    pruning_boundary_commit_fail_stop(format!("failed staging periodic PALW search-availability boundary: {err}"))
                });
            }
            self.palw_pruned_frontier_store
                .write()
                .set_batch(&mut batch, palw_snapshot)
                .unwrap_or_else(|err| pruning_boundary_commit_fail_stop(format!("failed staging periodic PALW boundary: {err}")));
            if let Some(snapshot) = overlay_snapshot {
                self.pruning_overlay_snapshot_store.write().set_batch(&mut batch, snapshot).unwrap_or_else(|err| {
                    pruning_boundary_commit_fail_stop(format!("failed staging periodic DNS overlay boundary: {err}"))
                });
            }

            // For archival nodes, keep the retention root in place
            let adjusted_retention_period_root = if self.config.is_archival {
                retention_period_root
            } else {
                let adjusted_retention_period_root = self.advance_retention_period_root(retention_period_root, new_pruning_point);
                pruning_point_write
                    .set_retention_period_root(&mut batch, adjusted_retention_period_root)
                    .unwrap_or_else(|err| pruning_boundary_commit_fail_stop(format!("failed staging periodic retention root: {err}")));
                adjusted_retention_period_root
            };

            if let Err(err) = self.db.write(batch) {
                pruning_boundary_commit_fail_stop(format!("periodic atomic pruning-boundary write failed: {err}"));
            }
            drop(pruning_point_write);
            drop(boundary_guard);

            trace!("New Pruning Point: {} | New Retention Period Root: {}", new_pruning_point, adjusted_retention_period_root);

            // Inform the user
            info!("Periodic pruning point movement: advancing from {} to {}", current_pruning_point, new_pruning_point);

            // Advance the pruning point utxoset to the state of the new pruning point using chain-block UTXO diffs
            if !self.advance_pruning_utxoset(current_pruning_point, new_pruning_point) {
                info!("Interrupted while advancing the pruning point UTXO set: Process is exiting");
                return;
            }
            info!("Updated the pruning point UTXO set");

            // Finally, prune data in the new pruning point past. Both PALW/DA and DNS
            // overlay sidecars were already committed atomically with the pruning-point
            // pointer, before UTXO advancement or deletion of their reconstruction rows.
            self.prune(new_pruning_point, adjusted_retention_period_root);
        }
    }

    fn advance_pruning_utxoset(&self, utxoset_position: BlockHash, new_pruning_point: BlockHash) -> bool {
        // If the latest pruning point is the result of an IBD catchup, it is guaranteed that the headers selected tip
        // is pruning_depth on top of it
        // but crucially it is not guaranteed *virtual* is of sufficient depth above it
        // internally the pruning process checks this process for virtual and fails otherwise
        // for this reason, pruning is held until virtual has advanced enough.
        if !self.confirm_pruning_depth_below_virtual(new_pruning_point) {
            return false;
        }

        for chain_block in self.reachability_service.forward_chain_iterator(utxoset_position, new_pruning_point, true).skip(1) {
            if self.is_consensus_exiting.load(Ordering::Relaxed) {
                return false;
            }
            // halt pruning if an unstable IBD state was initiated in the midst of it
            let pruning_meta_read = self.pruning_meta_stores.upgradable_read();

            if pruning_meta_read.is_in_transitional_ibd_state() {
                return false;
            }
            let mut pruning_meta_write = RwLockUpgradableReadGuard::upgrade(pruning_meta_read);

            let utxo_diff = self.utxo_diffs_store.get(chain_block).expect("chain blocks have utxo state");
            let mut batch = WriteBatch::default();
            pruning_meta_write.utxo_set.write_diff_batch(&mut batch, utxo_diff.as_ref()).unwrap();
            pruning_meta_write.set_utxoset_position(&mut batch, chain_block).unwrap();
            self.db.write(batch).unwrap();
            drop(pruning_meta_write);
        }

        if self.config.enable_sanity_checks {
            info!("Performing a sanity check that the new UTXO set has the expected UTXO commitment");
            self.assert_utxo_commitment(new_pruning_point);
        }
        true
    }

    // PR-9.5e: `pruning_point` is a block hash (BlockHash) despite the fn name; the
    // utxo_commitment read below is a 64-byte Hash64 (the MuHash accumulator commitment).
    fn assert_utxo_commitment(&self, pruning_point: BlockHash) {
        info!("Verifying the new pruning point UTXO commitment (sanity test)");
        let commitment = self.headers_store.get_header(pruning_point).unwrap().utxo_commitment;
        let mut multiset = MuHash::new();
        let pruning_meta_read = self.pruning_meta_stores.read();
        for (outpoint, entry) in pruning_meta_read.utxo_set.iterator().map(|r| r.unwrap()) {
            multiset.add_utxo(&outpoint, &entry);
        }
        assert_eq!(multiset.finalize(), commitment, "Updated pruning point utxo set does not match the header utxo commitment");
        info!("Pruning point UTXO commitment was verified correctly (sanity test)");
    }

    /// The most blocks the inverse-walk rebuild will revert before giving up. A
    /// bootstrap walk covers the retained (pp, tip] range, i.e. roughly the pruning
    /// depth; the bound is generous over that so a legitimately deep range succeeds,
    /// while a runaway (a broken diff chain) still terminates.
    #[cfg(feature = "evm")]
    const INVERSE_WALK_MAX_STEPS: usize = 4_000_000;

    /// §5.8: export the finalized EVM history a prune pass is about to reclaim into
    /// cold segments, and return the EVM-row delete FLOOR — below it rows may be
    /// deleted, at or above it they are held.
    ///
    /// Inert unless `--evm-segment-export=async` with a directory: then it archives
    /// up to `EXPORT_BATCH_BLOCKS` of the `(export_cursor, pruning_point]` range per
    /// pass, advances the manifest cursor, and returns `min(pp_evm, new_cursor)`.
    /// With export off it returns the pruning point, so pruning behaves exactly as
    /// before. Fails closed: any export error leaves the cursor where it was, so the
    /// floor holds the un-archived rows rather than letting them be deleted. L1
    /// pruning is never delayed — only the EVM-row delete range is bounded.
    #[cfg(feature = "evm")]
    fn evm_export_and_delete_floor(&self, pruning_point: BlockHash) -> u64 {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmNumberStoreReader, EvmStateDiffStoreReader};
        use kaspa_consensus_core::evm::evm_row_delete_floor;

        /// Bound the work (and the segment size) a single pruning pass does. The
        /// cursor catches up over successive passes; the floor holds the rest.
        const EXPORT_BATCH_BLOCKS: u64 = 50_000;

        let export = self.config.evm_segment_export;
        // Off / inert / no dir ⇒ floor is the pruning point (today's behaviour).
        if !export.is_active() || self.config.evm_activation_daa_score == u64::MAX {
            return u64::MAX;
        }
        let Some(dir) = self.config.evm_segment_dir.clone() else {
            warn!("[evm-export] --evm-segment-export=async but no --evm-segment-dir; export is inert and pruning is NOT gated");
            return u64::MAX;
        };
        let Ok(pp_header) = self.storage.evm_header_store.get(pruning_point) else {
            return u64::MAX; // pre-activation pruning point
        };
        let pp_evm = pp_header.evm_number;

        let mut manifest = self.storage.evm_cold_segment_manifest_store.read().get().unwrap_or_default();
        // The cursor is where the contiguous headers archive currently ends.
        let cursor = manifest
            .contiguous_range(kaspa_consensus_core::evm::ColdSegmentKind::Headers)
            .map(|(_, to)| to + 1)
            .unwrap_or(0);
        if cursor >= pp_evm {
            return evm_row_delete_floor(export, pp_evm, cursor); // already caught up
        }

        // Gather the next bounded batch of the finalized range in evm_number order.
        let to = pp_evm.min(cursor + EXPORT_BATCH_BLOCKS);
        let mut rows = Vec::new();
        for evm_number in cursor..to {
            let Ok(Some(block)) = self.storage.evm_number_store.get(evm_number) else {
                continue; // number not canonical / already gone — skip, cursor still advances past it
            };
            let Ok(header) = self.storage.evm_header_store.get(block) else { continue };
            let diff = self.storage.evm_state_diff_store.get(block).ok().flatten();
            rows.push(crate::processes::evm::cold_export::StateHistoryRow { evm_number, block, header, diff });
        }
        if rows.is_empty() {
            // Nothing canonical in this window (all detached/gone); advance the cursor
            // past it so we do not loop, but persist the advance via an empty-safe path.
            return evm_row_delete_floor(export, pp_evm, to);
        }

        match crate::processes::evm::cold_export::export_state_history_range(&dir, &rows, &mut manifest) {
            Ok(new_cursor) => {
                let mut batch = WriteBatch::default();
                if self.storage.evm_cold_segment_manifest_store.write().set_batch(&mut batch, manifest).is_ok()
                    && self.db.write(batch).is_ok()
                {
                    let lag = kaspa_consensus_core::evm::evm_export_lag_blocks(pp_evm, new_cursor);
                    info!("[evm-export] archived EVM history [{cursor}, {new_cursor}) to cold segments; export_lag_blocks={lag}");
                    return evm_row_delete_floor(export, pp_evm, new_cursor);
                }
                warn!("[evm-export] built segments for [{cursor}, {to}) but failed to persist the manifest; holding the rows");
                evm_row_delete_floor(export, pp_evm, cursor)
            }
            Err(e) => {
                warn!("[evm-export] export of [{cursor}, {to}) failed ({e}); holding these EVM rows (not deleting un-archived history)");
                evm_row_delete_floor(export, pp_evm, cursor)
            }
        }
    }

    /// Non-`evm` builds never gate.
    #[cfg(not(feature = "evm"))]
    fn evm_export_and_delete_floor(&self, _pruning_point: BlockHash) -> u64 {
        u64::MAX
    }

    /// Ensure `pruning_point` carries a §12 anchor, building one if it does not.
    ///
    /// Called immediately before a prune pass, which is the last instant at which
    /// the inputs exist: the previous pruning point's anchor and the diffs above it
    /// are about to be reclaimed. Each pass therefore hands the next one its floor.
    ///
    /// Never fatal. An anchor is serving/reconstruction convenience, and failing to
    /// build one must not stop pruning — but it IS logged as a warning naming the
    /// consequence, because the node has just lost the ability to serve pruned IBD
    /// for the EVM lane and the operator would otherwise learn that from a peer's
    /// root mismatch.
    #[cfg(feature = "evm")]
    fn ensure_evm_pruning_point_anchor(&self, pruning_point: BlockHash) {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmStateCheckpointV2StoreReader, EvmStateStoreReader};
        use kaspa_consensus_core::evm::EvmStateCheckpointV2;

        if self.config.evm_activation_daa_score == u64::MAX || !self.config.evm_history_mode.writes_state_history() {
            return;
        }
        // Pre-activation pruning point: nothing to anchor.
        let Ok(evm_header) = self.storage.evm_header_store.get(pruning_point) else { return };
        if EvmStateCheckpointV2StoreReader::has(&*self.storage.evm_state_checkpoint_v2_store, pruning_point).unwrap_or(false) {
            return;
        }

        // Resolve the state AT the pruning point. 206 first (a node that has not
        // retired it, or a freshly imported one), then the flat/reconstruct seed —
        // the same resolver block validation uses, so an anchor can never disagree
        // with what the executor would have seeded.
        let flat_head = self.storage.evm_latest_state_ptr_store.read().get().ok().flatten().map(|p| p.canonical_head);
        let snapshot = match self.storage.evm_state_store.get(pruning_point) {
            Ok(snapshot) => Some(snapshot),
            Err(_) => {
                // First the cheap forward path (206 retired but the pp is the flat
                // head, or an anchor still exists below it).
                let forward = crate::processes::evm::flat_or_reconstruct_parent_snapshot(
                    pruning_point,
                    flat_head,
                    &self.storage.evm_flat_account_store,
                    &self.storage.evm_code_store,
                    &self.storage.evm_header_store,
                    &self.storage.headers_store,
                    self.config.evm_activation_daa_score,
                    &self.storage.evm_state_checkpoint_v2_store,
                    &self.storage.evm_state_checkpoint_store,
                    &self.storage.evm_state_diff_store,
                )
                .ok()
                .map(|(snapshot, _source)| snapshot);
                // Then the inverse walk from the flat head. This is what BOOTSTRAPS a
                // node whose anchor material below the pruning point was already
                // reclaimed — the forward path cannot, because there is no anchor to
                // start from, which is exactly the state a node reaches once and then
                // cannot leave without this. Bounded by the pruning depth (this walks
                // the retained (pp, tip] diffs once), so it is a heavier fallback, not
                // the steady-state path.
                forward.or_else(|| {
                    flat_head.and_then(|fh| {
                        match crate::processes::evm::reconstruct_pruning_point_snapshot_verified(
                            pruning_point,
                            evm_header.state_root,
                            fh,
                            &self.storage.evm_flat_account_store,
                            &self.storage.evm_code_store,
                            &self.storage.evm_state_diff_store,
                            Self::INVERSE_WALK_MAX_STEPS,
                        ) {
                            Ok(snapshot) => {
                                info!("[evm-anchor] rebuilt pruning-point {pruning_point} state by inverse walk from the flat head");
                                Some(snapshot)
                            }
                            Err(e) => {
                                warn!("[evm-anchor] inverse-walk rebuild of {pruning_point} failed: {e}");
                                None
                            }
                        }
                    })
                })
            }
        };
        let Some(snapshot) = snapshot else {
            warn!(
                "[evm-anchor] could not build a §12 anchor for pruning point {pruning_point}: this node can no longer serve \
                 pruned IBD for the EVM lane, and historical state below the pruning point is unreconstructable. Run with \
                 --evm-history-mode=archive, or resync, to restore it."
            );
            return;
        };

        let checkpoint = EvmStateCheckpointV2::build(
            pruning_point,
            evm_header.evm_number,
            evm_header.state_root,
            &snapshot,
            self.config.evm_checkpoint_policy.codec,
        );
        let stored = checkpoint.stored_len();
        let mut batch = WriteBatch::default();
        if let Err(e) = self.storage.evm_state_checkpoint_v2_store.insert_batch(&mut batch, pruning_point, checkpoint) {
            warn!("[evm-anchor] pruning-point anchor write failed for {pruning_point}: {e}");
            return;
        }
        if let Err(e) = self.db.write(batch) {
            warn!("[evm-anchor] pruning-point anchor commit failed for {pruning_point}: {e}");
            return;
        }
        info!("[evm-anchor] pruning-point anchor at EVM #{} ({pruning_point}): {stored} B", evm_header.evm_number);
    }

    /// Non-`evm` builds have no EVM state to anchor.
    #[cfg(not(feature = "evm"))]
    fn ensure_evm_pruning_point_anchor(&self, _pruning_point: BlockHash) {}

    fn prune(&self, new_pruning_point: BlockHash, retention_period_root: BlockHash) {
        if self.config.is_archival {
            warn!("The node is configured as an archival node -- avoiding data pruning. Note this might lead to heavy disk usage.");
            return;
        }

        // BEFORE deleting anything: make sure the new pruning point has a §12
        // reconstruction anchor. This is the only moment the material to build one
        // still exists, and without it a `recent`-history node with 206 retired
        // becomes unable to serve pruned IBD for the EVM lane — observed on the live
        // testnet, where the serving node reconstructed the EMPTY state root for its
        // own pruning point. The pruning point is in `keep_blocks`, so its anchor
        // survives this pass and becomes the floor the next one reconstructs from.
        self.ensure_evm_pruning_point_anchor(new_pruning_point);
        // §5.8 interlock: export the finalized EVM history this pass will reclaim,
        // then compute the floor BELOW which EVM rows may be deleted. Inert (returns
        // u64::MAX) unless --evm-segment-export=async, so the per-block deletes below
        // are unchanged by default.
        let evm_delete_floor = self.evm_export_and_delete_floor(new_pruning_point);

        info!("Header and Block pruning: preparing proof and anticone data...");

        let proof = self.pruning_proof_manager.get_pruning_point_proof();
        let data = self
            .pruning_proof_manager
            .get_pruning_point_anticone_and_trusted_data()
            .expect("insufficient depth error is unexpected here");

        let genesis = self.past_pruning_points_store.get(0).unwrap();

        assert_eq!(new_pruning_point, proof[0].last().unwrap().hash);
        assert_eq!(new_pruning_point, data.anticone[0]);
        assert_eq!(genesis, self.config.genesis.hash);
        assert_eq!(genesis, proof.last().unwrap().last().unwrap().hash);

        // We keep full data for pruning point and its anticone, relations for DAA/GD
        // windows and pruning proof, and only headers for past pruning points
        let keep_blocks: BlockHashSet = data.anticone.iter().copied().collect();
        let mut keep_relations: BlockHashMap<BlockLevel> = std::iter::empty()
            .chain(data.anticone.iter().copied())
            .chain(data.daa_window_blocks.iter().map(|th| th.header.hash))
            .chain(data.ghostdag_blocks.iter().map(|gd| gd.hash))
            .chain(proof[0].iter().map(|h| h.hash))
            .map(|h| (h, 0)) // Mark block level 0 for all the above. Note that below we add the remaining levels
            .collect();
        let keep_headers: BlockHashSet = self.past_pruning_points();

        info!("Header and Block pruning: waiting for consensus write permissions...");

        // kaspa-pq (PALW DA + node-anchored search): the per-block DA and
        // search-availability stores use a full-row + link scheme — an idle block
        // stores a 64-byte link pointing DIRECTLY at the block owning the last full
        // row (its "anchor"). Anchors are monotone along a selected chain, so for
        // any LIVE block L above the pruning point, anchor(L) is either above the
        // pruning point too, or — when the state has not changed since before the
        // boundary — exactly the PRUNING POINT'S OWN anchor; no other below-boundary
        // full row can be referenced from above. Deleting that one row would dangle
        // every live link onto it, and the resolution path treats a dangling link as
        // a commit-fail-stop. So the walk below deletes these rows for every pruned
        // block EXCEPT each store's current boundary anchor. The skipped row is at
        // most one per store per pass and stops being referenced after the store's
        // next state change — a bounded remainder, not a leak.
        let palw_da_boundary_anchor = self.palw_da_store.read().state_and_anchor(new_pruning_point).ok().map(|(_, anchor)| anchor);
        let palw_search_boundary_anchor =
            self.palw_search_availability_store.read().state_and_anchor(new_pruning_point).ok().map(|(_, anchor)| anchor);

        let mut prune_guard = self.pruning_lock.blocking_write();

        info!("Starting Header and Block pruning...");

        {
            let mut counter = 0;
            let mut batch = WriteBatch::default();
            // At this point keep_relations only holds level-0 relations which is the correct filtering criteria for primary GHOSTDAG
            for kept in keep_relations.keys().copied() {
                let Some(ghostdag) = self.ghostdag_store.get_data(kept).optional().unwrap() else {
                    continue;
                };
                if ghostdag.unordered_mergeset().any(|h| !keep_relations.contains_key(&h)) {
                    let mut mutable_ghostdag: ExternalGhostdagData = ghostdag.as_ref().into();
                    mutable_ghostdag.mergeset_blues.retain(|h| keep_relations.contains_key(h));
                    mutable_ghostdag.mergeset_reds.retain(|h| keep_relations.contains_key(h));
                    mutable_ghostdag.blues_anticone_sizes.retain(|k, _| keep_relations.contains_key(k));
                    if !keep_relations.contains_key(&mutable_ghostdag.selected_parent) {
                        mutable_ghostdag.selected_parent = ORIGIN;
                    }
                    counter += 1;
                    self.ghostdag_store.update_batch(&mut batch, kept, &Arc::new(mutable_ghostdag.into())).unwrap();
                }
            }
            self.db.write(batch).unwrap();
            info!("Header and Block pruning: updated ghostdag data for {} blocks", counter);
        }

        // No need to hold the prune guard while we continue populating keep_relations
        drop(prune_guard);

        // Add additional levels only after filtering GHOSTDAG data via level 0
        for (level, level_proof) in proof.iter().enumerate().skip(1) {
            let level = level as BlockLevel;
            // We obtain the headers of the pruning point anticone (including the pruning point)
            // in order to mark all parents of anticone roots at level as not-to-be-deleted.
            // This optimizes multi-level parent validation (see ParentsManager)
            // by avoiding the deletion of high-level parents which might still be needed for future
            // header validation (avoiding the need for reference blocks; see therein).
            //
            // Notes:
            //
            // 1. Normally, such blocks would be part of the proof for this level, but here we address the rare case
            //    where there are a few such parallel blocks (since the proof only contains the past of the pruning point's
            //    selected-tip-at-level)
            // 2. We refer to the pp anticone as roots even though technically it might contain blocks which are not a pure
            //    antichain (i.e., some of them are in the past of others). These blocks only add redundant info which would
            //    be included anyway.
            let roots_parents_at_level = data
                .anticone
                .iter()
                .copied()
                .map(|hash| self.headers_store.get_header_with_block_level(hash).expect("pruning point anticone is not pruned"))
                .filter(|root| level > root.block_level) // If the root itself is at level, there's no need for its level-parents
                .flat_map(|root| self.parents_manager.parents_at_level(&root.header, level).iter().copied().collect_vec());
            for hash in level_proof.iter().map(|header| header.hash).chain(roots_parents_at_level) {
                if let Vacant(e) = keep_relations.entry(hash) {
                    // This hash was not added by any lower level -- mark it as affiliated with proof level `level`
                    e.insert(level);
                }
            }
        }

        prune_guard = self.pruning_lock.blocking_write();
        let mut lock_acquire_time = Instant::now();
        let mut reachability_read = self.reachability_store.upgradable_read();

        {
            // Start with a batch for pruning body tips and selected chain stores
            let mut batch = WriteBatch::default();

            // Prune tips which can no longer be merged by virtual.
            // By the prunality proof, any tip which isn't in future(pruning_point) will never be merged
            // by virtual and hence can be safely deleted
            let mut tips_write = self.body_tips_store.write();
            let pruned_tips = tips_write
                .get()
                .unwrap()
                .read()
                .iter()
                .copied()
                .filter(|&h| !reachability_read.try_is_dag_ancestor_of(new_pruning_point, h).unwrap())
                .collect_vec();
            tips_write.prune_tips_with_writer(BatchDbWriter::new(&mut batch), &pruned_tips).unwrap();
            if !pruned_tips.is_empty() {
                info!(
                    "Header and Block pruning: pruned {} tips: {}...{}",
                    pruned_tips.len(),
                    pruned_tips.iter().take(5.min(pruned_tips.len().div_ceil(2))).reusable_format(", "),
                    pruned_tips.iter().rev().take(5.min(pruned_tips.len() / 2)).reusable_format(", ")
                )
            }

            // Prune the selected chain index below the pruning point
            let mut selected_chain_write = self.selected_chain_store.write();
            // Temp — bug fix upgrade logic: the prev wrong logic might have pruned the new retention period root from the selected chain store,
            //                               hence we verify its existence first and only then proceed.
            // TODO (in upcoming versions): remove this temp condition
            if retention_period_root == new_pruning_point
                || selected_chain_write.get_by_hash(retention_period_root).optional().unwrap().is_some()
            {
                selected_chain_write.prune_below_point(BatchDbWriter::new(&mut batch), retention_period_root).unwrap();
            }

            // Flush the batch to the DB
            self.db.write(batch).unwrap();

            // Calling the drops explicitly after the batch is written in order to avoid possible errors.
            drop(selected_chain_write);
            drop(tips_write);
        }

        // Now we traverse the anti-future of the new pruning point starting from origin and going up.
        // The most efficient way to traverse the entire DAG from the bottom-up is via the reachability tree
        let mut queue = VecDeque::<BlockHash>::from_iter(reachability_read.get_children(ORIGIN).unwrap().iter().copied());
        let (mut counter, mut traversed) = (0, 0);
        info!("Header and Block pruning: starting traversal from: {} (genesis: {})", queue.iter().reusable_format(", "), genesis);
        while let Some(current) = queue.pop_front() {
            if reachability_read.try_is_dag_ancestor_of(retention_period_root, current).unwrap() {
                continue;
            }
            traversed += 1;
            // Obtain the tree children of `current` and push them to the queue before possibly being deleted below
            queue.extend(reachability_read.get_children(current).unwrap().iter());

            // If we have the lock for more than a few milliseconds, release and recapture to allow consensus progress during pruning
            if lock_acquire_time.elapsed() > Duration::from_millis(5) {
                drop(reachability_read);
                // An exit signal was received. Exit from this long running process.
                if self.is_consensus_exiting.load(Ordering::Relaxed) {
                    drop(prune_guard);
                    info!("Header and Block pruning interrupted: Process is exiting");
                    return;
                }
                prune_guard.blocking_yield();
                lock_acquire_time = Instant::now();
                reachability_read = self.reachability_store.upgradable_read();
            }

            if traversed % 1000 == 0 {
                info!("Header and Block pruning: traversed: {}, pruned {}...", traversed, counter);
            }

            // Remove window cache entries
            self.block_window_cache_for_difficulty.remove(&current);
            self.block_window_cache_for_past_median_time.remove(&current);

            if !keep_blocks.contains(&current) {
                let mut batch = WriteBatch::default();
                let mut relations_write = self.relations_store.write();
                let mut reachability_relations_write = self.reachability_relations_store.write();
                let mut staging_reachability_relations = StagingRelationsStore::new(&mut reachability_relations_write);
                let mut staging_reachability = StagingReachabilityStore::new(reachability_read);
                let mut statuses_write = self.statuses_store.write();

                // Prune data related to block bodies and UTXO state
                self.utxo_multisets_store.delete_batch(&mut batch, current).unwrap();
                self.utxo_diffs_store.delete_batch(&mut batch, current).unwrap();
                self.acceptance_data_store.delete_batch(&mut batch, current).unwrap();
                // kaspa-pq (ADR-0009 Addendum B §B.3(c)): prune the per-block
                // rewarded `(bond, epoch)` keys. A no-op for blocks that rewarded
                // nothing (no row), i.e. every block while the overlay is dormant.
                self.rewarded_epochs_store.delete_batch(&mut batch, current).unwrap();
                // kaspa-pq ADR-0040 §5.15.13 (gate G16): prune the per-block paid-`job_nullifier` row.
                // Sound because the reward-coordinate duplicate-work walk is bounded by
                // `PalwBatchAdmissionParams::paid_work_walk_bound_daa`, which is orders of magnitude
                // below the pruning depth — a pruned row can never be inside a live block's window.
                // `palw_paid_work_walk_stays_above_the_pruning_point` enforces that relation. A no-op
                // today: no row is ever written (no algo-4 source is acceptable on any preset).
                self.palw_paid_work_store.delete_batch(&mut batch, current).unwrap();
                // kaspa-pq ADR-0039 PALW (§15.2): prune the per-block active-nullifier window set. A
                // no-op while PALW is inert (no row). NOTE: the batch-scoped, content-addressed overlay
                // records (DbPalwStore leaf/manifest/certificate, keyed by batch_id/cert_hash — NOT
                // block-keyed) are NOT reclaimed here; `DbPalwStore::delete_batch_records` exists but is
                // not yet bound to a pruning-point batch-lifecycle sweep (activation TODO, D3). Inert
                // today (never written), so this is a growth item only on a PALW-activated net.
                self.palw_nullifier_store.delete_batch(&mut batch, current).unwrap();
                // kaspa-pq ADR-0039 PALW (§11.2): prune the per-block carried beacon state (R_E
                // recurrence). Block-keyed like the nullifier set → per-block delete. A no-op while
                // inert (no row). The epoch-keyed commit/reveal accumulator (PalwBeaconAccum) is NOT
                // pruned here — it is bounded by epoch count and, like `epoch_accumulator_store`, is
                // reclaimed by a later finalized-and-buried-epoch sweep. The seed recurrence only reads
                // the immediate selected parent, so pruning deep blocks never breaks it.
                self.palw_beacon_store.delete_state_batch(&mut batch, current).unwrap();
                self.palw_beacon_store.delete_accum_view_batch(&mut batch, current).unwrap();
                // kaspa-pq ADR-0039 PALW (§16.3): prune the per-block carried lane-difficulty bits.
                // Block-keyed, depth-1 recurrence (only the immediate selected parent's bits are read
                // as the HOLD source), so pruning deep blocks never breaks it. No-op while inert.
                self.palw_lane_bits_store.delete_batch(&mut batch, current).unwrap();
                // kaspa-pq ADR-0039 PALW (§18.2, C5 option B): prune the per-block carried batch-view.
                // Block-keyed like the nullifier set (view(B)=view(SP(B))⊕Δ(mergeset(B)) reads only the
                // immediate selected parent), so pruning deep blocks never breaks it. No-op while inert.
                self.palw_overlay_view_store.delete_batch(&mut batch, current).unwrap();
                // kaspa-pq (PALW DA + node-anchored search): prune the per-block carried DA state and
                // search-availability rows (full rows AND their 64-byte links). These were the last
                // per-block PALW stores missing from this walk — on a fence-0 preset they are written
                // for EVERY block and had no deletion path anywhere. Each store's current boundary
                // anchor is skipped; see the anchor computation above the walk for why deleting it
                // would dangle live links and fail-stop the overlay resolution path.
                if palw_da_boundary_anchor != Some(current) {
                    self.palw_da_store.write().delete_state_batch(&mut batch, current).unwrap();
                }
                if palw_search_boundary_anchor != Some(current) {
                    self.palw_search_availability_store.write().delete_state_batch(&mut batch, current).unwrap();
                }
                // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): prune the per-block
                // validator quality sub-pool sibling. A no-op while inert (no row).
                // The per-epoch `epoch_accumulator_store` is keyed by epoch (not
                // block), so it is not pruned here — bounded by epoch count and
                // inert today; pruning finalized-and-buried epochs is a later slice.
                self.block_quality_pool_store.delete_batch(&mut batch, current).unwrap();
                // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): prune the per-block reserve balance.
                // A no-op while inert (no row). The drip recurrence only reads the immediate
                // (recent) selected parent, so pruning deep blocks never breaks it.
                self.reserve_balance_store.delete_batch(&mut batch, current).unwrap();
                self.block_transactions_store.delete_batch(&mut batch, current).unwrap();
                // kaspa-pq ADR-0020 (EVM lane): prune this pruned block's per-block
                // EVM data (audit H-01 — without this the EVM stores grew O(state ×
                // blocks) and were never reclaimed). All keyed by the L1 block hash;
                // a no-op (delete-of-absent) on nets/blocks with no EVM rows, so it
                // is inert on mainnet/simnet and on pre-activation blocks. The
                // executor only ever seeds from the selected-parent snapshot (a
                // recent, kept block) and the pruning-point anchor snapshot is in
                // keep_blocks, so deleting buried blocks' EVM state is safe; archival
                // (`is_archival`) nodes skip pruning entirely and retain everything.
                //
                // The full snapshot (206), payload, receipts and trace are ALWAYS
                // reclaimed here. §12: whether this block's EVM STATE remains
                // queryable past pruning depends on `--evm-history-mode`:
                //   - `archive`: keep the header + archive diff (220) + checkpoint
                //     (221) so `reconstruct_evm_state_at` can rebuild this pruned
                //     block's state (the dropped snapshot is replaced by replaying
                //     the checkpoint forward through the diffs).
                //   - `head`/`recent`: reclaim the header + diff + checkpoint too
                //     (a no-op in `head`, which writes no diffs) — state history is
                //     kept only for unpruned blocks.
                // The content-addressed code store (222) is NEVER per-block pruned:
                // a `code_hash` is shared by every block that references that code,
                // so deleting it on one block's pruning would corrupt others.
                // §5.8 interlock: hold this block's EVM history rows if its
                // evm_number is at or above the delete floor — i.e. the export has
                // not yet archived it. `u64::MAX` (export off) never holds, so this
                // is a no-op on the default path. Blocks with no EVM header read as
                // evm_number 0 and are always below the floor (nothing to hold).
                let evm_held = {
                    use crate::model::stores::evm::EvmHeaderStoreReader;
                    self.evm_header_store.get(current).map(|h| h.evm_number >= evm_delete_floor).unwrap_or(false)
                };
                if !evm_held {
                    self.evm_state_store.delete_batch(&mut batch, current).unwrap();
                    self.evm_payload_store.delete_batch(&mut batch, current).unwrap();
                    self.evm_receipts_store.delete_batch(&mut batch, current).unwrap();
                    // §11: the per-block trace replay plan is reclaimed with the rest of
                    // the block's EVM rows (a trace cannot outlive its pre-state snapshot).
                    self.evm_trace_store.delete_batch(&mut batch, current).unwrap();
                    if self.config.evm_history_mode.retains_state_history_past_pruning() {
                        // Archive: keep header + diff + checkpoint for historical reconstruction.
                    } else {
                        self.evm_header_store.delete_batch(&mut batch, current).unwrap();
                        self.evm_state_diff_store.delete_batch(&mut batch, current).unwrap();
                        // Both anchor formats: the legacy 221 rows a pre-v2 database still
                        // carries, and the v2 anchors (223) this node writes.
                        self.evm_state_checkpoint_store.delete_batch(&mut batch, current).unwrap();
                        self.evm_state_checkpoint_v2_store.delete_batch(&mut batch, current).unwrap();
                    }
                }

                if let Some(&affiliated_proof_level) = keep_relations.get(&current) {
                    if statuses_write.get(current).optional().unwrap().is_some_and(|s| s.is_valid()) {
                        // We set the status to header-only only if it was previously set to a valid
                        // status. This is important since some proof headers might not have their status set
                        // and we would like to preserve this semantic (having a valid status implies that
                        // other parts of the code assume the existence of GD data etc.)
                        statuses_write.set_batch(&mut batch, current, StatusHeaderOnly).unwrap();
                    }

                    // delete relations and ghostdag unless current is in level 0 of the pruning proof
                    if affiliated_proof_level > 0 {
                        let mut staging_relations = StagingRelationsStore::new(&mut relations_write);
                        relations::delete_level_relations(MemoryWriter, &mut staging_relations, current).optional().unwrap();
                        staging_relations.commit(&mut batch).unwrap();
                        self.ghostdag_store.delete_batch(&mut batch, current).optional().unwrap();
                    }
                    // while we keep headers for keep relation blocks regardless,
                    // some of those relations blocks may accidentally have a pruning sample stored,
                    // delete those samples unless the block is a pruning block itself
                    if !keep_headers.contains(&current) {
                        self.pruning_samples_store.delete_batch(&mut batch, current).unwrap();
                    }
                } else {
                    // Count only blocks which get fully pruned including DAG relations
                    counter += 1;
                    // Prune data related to headers: relations, reachability, ghostdag
                    let mergeset = relations::delete_reachability_relations(
                        MemoryWriter, // Both stores are staging so we just pass a dummy writer
                        &mut staging_reachability_relations,
                        &staging_reachability,
                        current,
                    );
                    reachability::delete_block(&mut staging_reachability, current, &mut mergeset.iter().copied()).unwrap();
                    let mut staging_relations = StagingRelationsStore::new(&mut relations_write);
                    relations::delete_level_relations(MemoryWriter, &mut staging_relations, current).optional().unwrap();
                    staging_relations.commit(&mut batch).unwrap();

                    self.ghostdag_store.delete_batch(&mut batch, current).optional().unwrap();

                    // Remove additional header related data
                    self.daa_excluded_store.delete_batch(&mut batch, current).unwrap();
                    self.depth_store.delete_batch(&mut batch, current).unwrap();
                    // Remove status completely
                    statuses_write.delete_batch(&mut batch, current).unwrap();

                    if !keep_headers.contains(&current) {
                        // Prune the actual headers
                        self.headers_store.delete_batch(&mut batch, current).unwrap();

                        // We want to keep the pruning sample from POV for past pruning points
                        // so that pruning point queries keep working for blocks right after the current
                        // pruning point (keep_headers contains the past pruning points)
                        self.pruning_samples_store.delete_batch(&mut batch, current).unwrap();
                    }
                }

                let reachability_write = staging_reachability.commit(&mut batch).unwrap();
                staging_reachability_relations.commit(&mut batch).unwrap();

                // Flush the batch to the DB
                self.db.write(batch).unwrap();

                // Calling the drops explicitly after the batch is written in order to avoid possible errors.
                drop(reachability_write);
                drop(statuses_write);
                drop(reachability_relations_write);
                drop(relations_write);

                reachability_read = self.reachability_store.upgradable_read();
            }
        }

        drop(reachability_read);

        // Header/body deletion above is fully committed before classifying anti-spam rows. This
        // avoids deciding against a header that is still present only in the same uncommitted batch.
        // The sweep is fail-closed: any read/closure failure retains every row.
        self.sweep_pruned_palw_spam_rows();
        drop(prune_guard);

        info!("Header and Block pruning completed: traversed: {}, pruned {}", traversed, counter);
        info!(
            "Header and Block pruning stats: proof size: {}, pruning point and anticone: {}, unique headers in proof and windows: {}, pruning points in history: {}",
            proof.iter().map(|l| l.len()).sum::<usize>(),
            keep_blocks.len(),
            keep_relations.len(),
            keep_headers.len()
        );

        if self.config.enable_sanity_checks {
            self.assert_proof_rebuilding(proof, new_pruning_point);
            self.assert_data_rebuilding(data, new_pruning_point);
        }

        {
            // Set the retention checkpoint to the new retention root only after we successfully pruned its past
            let mut pruning_point_write = self.pruning_point_store.write();
            let mut batch = WriteBatch::default();
            pruning_point_write.set_retention_checkpoint(&mut batch, retention_period_root).unwrap();
            self.db.write(batch).unwrap();
            drop(pruning_point_write);
        }
    }

    /// Reclaim Header-v4 accumulator rows which are outside every retained-header and current
    /// pruning-snapshot closure. All candidates are preflighted before the first cache-backed delete,
    /// and the deletes commit in one RocksDB batch. This deliberately runs only after ordinary prune
    /// batches complete; boundary/catch-up replacement may leave old support temporarily, but never
    /// guesses that a header-only/proof/side-fork parent is dead.
    fn sweep_pruned_palw_spam_rows(&self) {
        if self.config.params.palw_spam.is_inert() {
            return;
        }
        let Some(snapshot) = self.virtual_processor.pruning_point_palw_snapshot() else {
            warn!("PALW spam sweep retained all rows because the current pruning snapshot is unavailable or invalid");
            return;
        };
        let Some(frontier) = snapshot.payload.spam_accumulator.as_ref() else {
            warn!("PALW spam sweep retained all rows because the active pruning snapshot has no spam frontier");
            return;
        };

        let rows = match self.palw_spam_store.iter().collect::<Result<Vec<_>, _>>() {
            Ok(rows) => rows,
            Err(err) => {
                warn!("PALW spam sweep retained all rows because store enumeration failed: {err}");
                return;
            }
        };
        let mut closure_tips = Vec::new();
        for (hash, state) in &rows {
            if let Err(err) = state.validate_shape() {
                warn!("PALW spam sweep retained all rows because row {hash} is malformed: {err}");
                return;
            }
            match self.headers_store.has(*hash) {
                Ok(true) => closure_tips.push(*hash),
                Ok(false) => {}
                Err(err) => {
                    warn!("PALW spam sweep retained all rows because header-presence lookup failed for {hash}: {err}");
                    return;
                }
            }
        }
        // The PP is a closure tip. Its transported support rows are pinned facts inside that exact
        // closure, not independent tips demanding another checkpoint below the import floor.
        closure_tips.push(snapshot.payload.pruning_point);
        let pinned_support = frontier.support_rows.iter().map(|row| row.block_hash);

        let reclaim = match palw_spam_reclaim_candidates(
            self.palw_spam_store.as_ref(),
            rows.iter().map(|(hash, _)| *hash),
            closure_tips,
            pinned_support,
            self.config.params.palw_spam.window_daa,
        ) {
            Ok(reclaim) => reclaim,
            Err(err) => {
                warn!("PALW spam sweep retained all rows because bounded closure preflight failed: {err}");
                return;
            }
        };
        if reclaim.is_empty() {
            return;
        }

        let mut batch = WriteBatch::default();
        for hash in &reclaim {
            self.palw_spam_store.delete_batch(&mut batch, *hash).unwrap_or_else(|err| {
                pruning_boundary_commit_fail_stop(format!("failed staging PALW spam-history reclaim for {hash}: {err}"))
            });
        }
        if let Err(err) = self.db.write(batch) {
            pruning_boundary_commit_fail_stop(format!("PALW spam-history reclaim batch failed: {err}"));
        }
        info!("Reclaimed {} PALW spam accumulator rows outside every retained closure", reclaim.len());
    }

    /// Adjusts the retention period root to latest pruning point sample that covers the retention period.
    /// This is the pruning point sample B such that B.timestamp <= retention_period_days_ago. This may return the old hash if
    /// the retention period cannot be covered yet with the node's current history.
    ///
    /// This function is expected to be called only when a new pruning point is determined and right before
    /// doing any pruning. Pruning point must be the new pruning point this node is advancing to.
    ///
    /// The returned retention_period_root is guaranteed to be in past(pruning_point) or the pruning point itself.
    pub fn advance_retention_period_root(&self, retention_period_root: BlockHash, pruning_point: BlockHash) -> BlockHash {
        match self.config.retention_period_days {
            // If the retention period wasn't set, immediately default to the pruning point.
            None => pruning_point,
            Some(retention_period_days) => {
                // The retention period in milliseconds we need to cover
                // Note: If retention period is set to an amount lower than what the new pruning point would cover
                // this function will simply return the new pruning point. The new pruning point passed as an argument
                // to this function serves as a clamp.
                let retention_period_ms = (retention_period_days * 86400.0 * 1000.0).ceil() as u64;

                // The target timestamp we would like to find a point below
                let sink_timestamp_as_current_time = self.get_sink_timestamp();
                let retention_period_root_ts_target = sink_timestamp_as_current_time.saturating_sub(retention_period_ms);

                // Iterate from the new pruning point to the prev retention root and search for the first point with enough days above it.
                // Note that prev retention root is always a past pruning point, so we can iterate via pruning samples until we reach it.
                let mut new_retention_period_root = pruning_point;

                trace!(
                    "Adjusting the retention period root to cover the required retention period. Target timestamp: {}",
                    retention_period_root_ts_target,
                );

                while new_retention_period_root != retention_period_root {
                    let block = new_retention_period_root;

                    let timestamp = self.headers_store.get_timestamp(block).unwrap();
                    trace!("block | timestamp = {} | {}", block, timestamp);
                    if timestamp <= retention_period_root_ts_target {
                        trace!("block {} timestamp {} >= {}", block, timestamp, retention_period_root_ts_target);
                        // We are now at a pruning point that is at or below our retention period target
                        break;
                    }

                    new_retention_period_root = self.pruning_samples_store.pruning_sample_from_pov(block).unwrap();
                }

                new_retention_period_root
            }
        }
    }

    fn get_sink_timestamp(&self) -> u64 {
        self.headers_store.get_timestamp(self.get_sink()).unwrap()
    }

    fn get_sink(&self) -> BlockHash {
        self.lkg_virtual_state.load().ghostdag_data.selected_parent
    }

    fn past_pruning_points(&self) -> BlockHashSet {
        (0..self.pruning_point_store.read().pruning_point_index().unwrap())
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .collect()
    }

    fn confirm_pruning_depth_below_virtual(&self, pruning_point: BlockHash) -> bool {
        // Fresh pruned-IBD fix (2026-06-26): the virtual state is not established until the
        // virtual processor resolves it, so a missing VirtualState here means the pruning depth
        // below virtual is not yet confirmable. DEFER pruning (return false) instead of
        // panicking with KeyNotFound(VirtualState) -- the worker retries on its next Process
        // message, by which time virtual has advanced. Same fallible-read posture ab6f90e
        // applied to the virtual-processor reachability reads (it missed this one).
        let Ok(virtual_state) = self.virtual_stores.read().state.get() else {
            return false;
        };
        let pp_bs = self.headers_store.get_blue_score(pruning_point).unwrap();
        virtual_state.ghostdag_data.blue_score >= pp_bs + self.config.params.pruning_depth()
    }

    fn assert_proof_rebuilding(&self, ref_proof: Arc<PruningPointProof>, new_pruning_point: BlockHash) {
        info!("Rebuilding the pruning proof after pruning data (sanity test)");
        let built_proof = self.pruning_proof_manager.build_pruning_point_proof(new_pruning_point);
        if ref_proof.len() != built_proof.len() {
            panic!("Rebuilt proof does not match the original one ({} ref vs. {} rebuilt levels)", ref_proof.len(), built_proof.len());
        }
        for (i, (ref_level, built_level)) in ref_proof.iter().zip(built_proof.iter()).enumerate() {
            if ref_level.iter().map(|h| h.hash).ne(built_level.iter().map(|h| h.hash)) {
                panic!("Rebuilt proof for level {} does not match the original one", i);
            }
        }
        info!("Proof was rebuilt successfully following pruning");
    }

    fn assert_data_rebuilding(&self, ref_data: Arc<PruningPointTrustedData>, new_pruning_point: BlockHash) {
        info!("Rebuilding pruning point trusted data (sanity test)");
        let virtual_state = self.lkg_virtual_state.load();
        let built_data = self
            .pruning_proof_manager
            .calculate_pruning_point_anticone_and_trusted_data(new_pruning_point, virtual_state.parents.iter().copied());
        assert_eq!(
            ref_data.anticone.iter().copied().collect::<BlockHashSet>(),
            built_data.anticone.iter().copied().collect::<BlockHashSet>()
        );
        assert_eq!(
            ref_data.daa_window_blocks.iter().map(|th| th.header.hash).collect::<BlockHashSet>(),
            built_data.daa_window_blocks.iter().map(|th| th.header.hash).collect::<BlockHashSet>()
        );
        assert_eq!(
            ref_data.ghostdag_blocks.iter().map(|gd| gd.hash).collect::<BlockHashSet>(),
            built_data.ghostdag_blocks.iter().map(|gd| gd.hash).collect::<BlockHashSet>()
        );
        info!("Trusted data was rebuilt successfully following pruning");
    }
}

#[cfg(test)]
mod palw_snapshot_recovery_tests {
    use super::{PalwSnapshotRecoveryPlan, palw_snapshot_recovery_plan, pruning_boundary_source_rows_pruned};
    use crate::model::stores::{
        palw_da::{DbPalwDaStore, PalwDaStoreReader},
        palw_pruned_frontier::{DbPalwPrunedFrontierStore, PalwPrunedFrontierStoreReader},
        pruning::{DbPruningStore, PruningStoreReader},
        pruning_overlay_snapshot::{DbPruningPointOverlaySnapshotStore, PruningPointOverlaySnapshotStoreReader},
    };
    use kaspa_consensus_core::{
        BlockHash,
        dns_finality::{OverlaySnapshot, PruningPointOverlaySnapshot},
        palw::{PalwPrunedFrontierV1, da::PalwDaPruningSnapshotV1, da::PalwDaStateV1},
        palw_pruned_frontier::{PALW_PRUNING_SNAPSHOT_VERSION, PalwPruningPointSnapshotPayloadV1, PalwPruningPointSnapshotV1},
    };
    use kaspa_database::{
        create_temp_db,
        prelude::{CachePolicy, ConnBuilder},
    };
    use rocksdb::WriteBatch;

    fn hash(byte: u8) -> BlockHash {
        BlockHash::from_bytes([byte; 64])
    }

    #[test]
    fn missing_or_corrupt_snapshot_repairs_only_while_source_rows_remain() {
        assert_eq!(palw_snapshot_recovery_plan(true, false, false), PalwSnapshotRecoveryPlan::RebuildFromRetainedRows);
        assert_eq!(palw_snapshot_recovery_plan(true, false, true), PalwSnapshotRecoveryPlan::FailClosed);
    }

    #[test]
    fn valid_or_genesis_boundary_needs_no_recovery() {
        assert_eq!(palw_snapshot_recovery_plan(true, true, true), PalwSnapshotRecoveryPlan::None);
        assert_eq!(palw_snapshot_recovery_plan(false, false, true), PalwSnapshotRecoveryPlan::None);
    }

    #[test]
    fn archival_boundary_keeps_repair_sources_even_when_checkpoint_reaches_root() {
        let root = hash(7);
        assert!(!pruning_boundary_source_rows_pruned(true, root, root));
        assert!(matches!(
            palw_snapshot_recovery_plan(true, false, pruning_boundary_source_rows_pruned(true, root, root)),
            PalwSnapshotRecoveryPlan::RebuildFromRetainedRows
        ));
    }

    #[test]
    fn completed_non_archival_prune_is_not_rebuilt_from_deleted_rows() {
        let root = hash(8);
        assert!(pruning_boundary_source_rows_pruned(false, root, root));
        assert!(!pruning_boundary_source_rows_pruned(false, hash(9), root));
    }

    #[test]
    fn pruning_pointer_palw_da_and_dns_sidecars_are_one_db_batch() {
        use crate::model::stores::palw_search_availability::{DbPalwSearchAvailabilityStore, PalwSearchAvailabilityStoreReader};
        use kaspa_consensus_core::palw::search_snapshot::{
            PALW_SEARCH_SNAPSHOT_STATE_VERSION_V1, PalwSearchAvailabilityStateV1, PalwSearchPruningSnapshotV1,
        };
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let pruning_point = hash(0x31);
        let da = PalwDaPruningSnapshotV1 { version: 1, pruning_point, state: PalwDaStateV1::default() };
        let search = PalwSearchPruningSnapshotV1 {
            version: PALW_SEARCH_SNAPSHOT_STATE_VERSION_V1,
            pruning_point,
            state: PalwSearchAvailabilityStateV1::default(),
        };
        let palw = PalwPruningPointSnapshotV1::new(PalwPruningPointSnapshotPayloadV1 {
            version: PALW_PRUNING_SNAPSHOT_VERSION,
            pruning_point,
            pruning_point_daa_score: 500,
            paid_work_window_daa: 0,
            frontier: PalwPrunedFrontierV1::default(),
            beacon_accumulator: None,
            spam_accumulator: None,
            da_snapshot: Some(da.clone()),
            search_availability_snapshot: Some(search.clone()),
            active_batches: vec![],
            provider_bonds: vec![],
            paid_work: vec![],
        });
        let overlay = PruningPointOverlaySnapshot { pruning_point, snapshot: OverlaySnapshot::default() };

        let mut pruning_store = DbPruningStore::new(db.clone());
        let mut palw_store = DbPalwPrunedFrontierStore::new(db.clone());
        let mut da_store = DbPalwDaStore::new(db.clone(), CachePolicy::Count(8));
        let mut search_store = DbPalwSearchAvailabilityStore::new(db.clone(), CachePolicy::Count(8));
        let mut overlay_store = DbPruningPointOverlaySnapshotStore::new(db.clone());
        let pruning_observer = pruning_store.clone_with_new_cache();
        let palw_observer = palw_store.clone_with_new_cache();
        let da_observer = da_store.clone_with_new_cache(CachePolicy::Count(8));
        let search_observer = search_store.clone_with_new_cache(CachePolicy::Count(8));
        let overlay_observer = overlay_store.clone_with_new_cache();

        let mut batch = WriteBatch::default();
        pruning_store.set_batch(&mut batch, pruning_point, 4).unwrap();
        da_store.set_pruning_snapshot_batch(&mut batch, &da).unwrap();
        search_store.set_pruning_snapshot_batch(&mut batch, &search).unwrap();
        palw_store.set_batch(&mut batch, palw.clone()).unwrap();
        overlay_store.set_batch(&mut batch, overlay.clone()).unwrap();

        assert!(pruning_observer.pruning_point().is_err());
        assert!(palw_observer.get().is_err());
        assert!(da_observer.pruning_snapshot().is_err());
        assert!(search_observer.pruning_snapshot().is_err());
        assert!(overlay_observer.get().is_err());

        db.write(batch).unwrap();
        assert_eq!(pruning_observer.pruning_point().unwrap(), pruning_point);
        assert_eq!(palw_observer.get().unwrap(), palw);
        assert_eq!(da_observer.pruning_snapshot().unwrap(), da);
        assert_eq!(search_observer.pruning_snapshot().unwrap(), search);
        assert_eq!(overlay_observer.get().unwrap(), overlay);
    }
}
