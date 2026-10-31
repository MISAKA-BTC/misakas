//! A pruning caller must prove exact-point PALW serving readiness before deleting
//! the delta history used to reconstruct it. Local faults never invalidate work.
use super::*;
use crate::processes::palw_state_walk::PalwStateWalkError;
use kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1;
use kaspa_database::prelude::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum PalwPruningCaptureFaultV1 {
    #[error("PALW pruning snapshot context: {0}")]
    Context(&'static str),
    #[error("PALW pruning snapshot store: {0}")]
    Store(StoreError),
    #[error("PALW pruning snapshot native capacity: {0}")]
    NativeResource(NativeCoverageV1),
    #[error("PALW pruning snapshot walk: {0}")]
    Walk(PalwStateWalkError),
    #[error("overlay pruning snapshot: {0}")]
    Overlay(RuleError),
}
impl PalwPruningCaptureFaultV1 {
    fn store(error: StoreError) -> Self {
        match error {
            StoreError::LocalResourceUnavailable(reason) => Self::NativeResource(NativeCoverageV1::unsupported::<
                kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
            >(
                "PALW pruning snapshot store capacity", &reason
            )),
            other => Self::Store(other),
        }
    }
    fn walk(error: PalwStateWalkError) -> Self {
        match error.native_capacity_v1() {
            Some(capacity) => Self::NativeResource(capacity),
            None => Self::Walk(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PruningCaptureContextV1 {
    pruning_point: BlockHash,
    epoch: u64,
    active: Option<kaspa_consensus_core::pruning::PruningImportSessionV1>,
    utxos_started: bool,
    utxos_imported: bool,
    stable: bool,
}

impl VirtualStateProcessor {
    // A short, nonblocking generation read. An import/point change is a local
    // retry, never permission to publish a snapshot from another generation.
    fn pruning_capture_context_v1(&self) -> Result<PruningCaptureContextV1, PalwPruningCaptureFaultV1> {
        let _import =
            self.pruning_import_session_lock.try_lock().ok_or(PalwPruningCaptureFaultV1::Context("import mutation in progress"))?;
        let pruning = self.pruning_point_store.try_read().ok_or(PalwPruningCaptureFaultV1::Context("pruning point is changing"))?;
        let meta = self.pruning_meta_stores.try_read().ok_or(PalwPruningCaptureFaultV1::Context("import metadata is changing"))?;
        let session = meta.import_session().map_err(PalwPruningCaptureFaultV1::store)?;
        Ok(PruningCaptureContextV1 {
            pruning_point: pruning.pruning_point().map_err(PalwPruningCaptureFaultV1::store)?,
            epoch: session.epoch,
            active: session.active,
            utxos_started: session.utxos_started,
            utxos_imported: session.utxos_imported,
            stable: meta.try_pruning_utxoset_stable_flag().map_err(PalwPruningCaptureFaultV1::store)?,
        })
    }

    fn check_pruning_capture_context_v1(&self, expected: &PruningCaptureContextV1) -> Result<(), PalwPruningCaptureFaultV1> {
        if &self.pruning_capture_context_v1()? != expected {
            return Err(PalwPruningCaptureFaultV1::Context("pruning/import generation changed during capture"));
        }
        Ok(())
    }

    fn require_served_pruning_point_v1(
        &self,
        point: BlockHash,
        carriage: &kaspa_consensus_core::palw_native_resource_v1::NativeStateCarriageV1,
    ) -> Result<(), PalwPruningCaptureFaultV1> {
        match carriage.last_point.as_ref() {
            Some(last) if last.block == point => Ok(()),
            // Genesis is the pre-transition state; its established carriage may have no last point.
            None if point == self.genesis.hash => Ok(()),
            _ => Err(PalwPruningCaptureFaultV1::Context("served carriage last point differs")),
        }
    }

    pub(super) fn capture_pruning_point_palw_state_checked_v1(&self, point: BlockHash) -> Result<(), PalwPruningCaptureFaultV1> {
        self.capture_pruning_point_palw_state_while_v1(point, || true)
    }

    /// The checkpoint is a local cancellation/progress door, not a consensus
    /// policy. Each historical delta still uses the unchanged checked replay.
    pub(super) fn capture_pruning_point_palw_state_while_v1(
        &self,
        point: BlockHash,
        keep_running: impl Fn() -> bool,
    ) -> Result<(), PalwPruningCaptureFaultV1> {
        let Some(params) = self.palw_state_params_v2.as_ref() else { return Ok(()) };
        let started = std::time::Instant::now();
        info!("PALW: pruning snapshot capture {point} started (detached reader)");
        let context = self.pruning_capture_context_v1()?;
        // DbPalwStateV2Store clones share the same DB/config and immutable delta
        // rows. Pin/decode owned native state without holding its GLOBAL lock:
        // the dense-floor incident held it for 46 minutes of full-state replay,
        // blocking virtual commits, then PALW readers behind their queued writer.
        let detached =
            self.palw_state_v2_store.try_read().ok_or(PalwPruningCaptureFaultV1::Context("PALW store is changing"))?.clone();
        let checkpoint = || {
            if !keep_running() {
                return Err(PalwPruningCaptureFaultV1::Context("pruning capture interrupted"));
            }
            self.check_pruning_capture_context_v1(&context)
        };
        checkpoint()?;
        // Exact durable snapshots remain reusable after partially completed GC.
        if let Some(carriage) =
            detached.load_pruning_snapshot_carriage_cached(params, point).map_err(PalwPruningCaptureFaultV1::store)?
        {
            self.require_served_pruning_point_v1(point, &carriage)?;
            checkpoint()?;
            return Ok(());
        }
        // Choose from compact durable identities BEFORE materializing state.
        // A previous exact checkpoint can be hundreds of deltas from the next
        // point while the live tip is tens of thousands of dense-floor rows away.
        let tip_key = detached
            .tip_key()
            .map_err(PalwPruningCaptureFaultV1::store)?
            .ok_or(PalwPruningCaptureFaultV1::Context("active PALW tip is absent"))?;
        if !self.reachability_service.has_reachability_data(point) || !self.reachability_service.has_reachability_data(tip_key.0) {
            return Err(PalwPruningCaptureFaultV1::Context("snapshot anchor/target reachability is absent"));
        }
        let tip_path = self.dag_traversal_manager.calculate_chain_path(tip_key.0, point, None);
        let tip_hops = tip_path.removed.len() + tip_path.added.len();
        let prior = detached.pruning_snapshot_key().map_err(PalwPruningCaptureFaultV1::store)?;
        let prior_path = prior
            .filter(|(block, _)| self.reachability_service.has_reachability_data(*block))
            .map(|key| (key, self.dag_traversal_manager.calculate_chain_path(key.0, point, None)));
        let (anchor_key, path, from_checkpoint) = match prior_path {
            Some((key, path)) if path.removed.len() + path.added.len() < tip_hops => (key, path, true),
            _ => (tip_key, tip_path, false),
        };
        let (tip, mut state) =
            if from_checkpoint { detached.load_pruning_snapshot_retained_v1(params) } else { detached.load_tip_retained_v1(params) }
                .map_err(PalwPruningCaptureFaultV1::store)?
                .ok_or(PalwPruningCaptureFaultV1::Context("active PALW tip is absent"))?;
        // The compact identity selects cost only. Its state must still decode,
        // rebuild and match its recorded root through the ordinary native loader.
        // A concurrent changed row is retry, not an unauthenticated new anchor.
        if tip != anchor_key.0 || state.state_root() != anchor_key.1 {
            return Err(PalwPruningCaptureFaultV1::Context("chosen snapshot anchor identity changed"));
        }
        checkpoint()?;
        info!(
            "PALW: pruning snapshot {point} loaded {} anchor {tip} after {} ms ({} hops; live-tip path {tip_hops} hops)",
            if from_checkpoint { "checkpoint" } else { "tip" },
            started.elapsed().as_millis(),
            path.removed.len() + path.added.len()
        );
        if tip != point {
            info!(
                "PALW: reconstructing pruning snapshot {point} from {tip}: {} revert / {} apply deltas (detached reader)",
                path.removed.len(),
                path.added.len()
            );
            let total = path.removed.len() + path.added.len();
            let mut completed = 0usize;
            let mut progress_at = std::time::Instant::now();
            for (blocks, revert) in [(&path.removed, true), (&path.added, false)] {
                for block in blocks.iter() {
                    checkpoint()?;
                    let delta = detached
                        .delta_of_retained_v1(*block)
                        .map_err(|error| {
                            let error = match error {
                                StoreError::KeyNotFound(_) => PalwStateWalkError::MissingDelta(*block),
                                other => PalwStateWalkError::Store(other),
                            };
                            PalwPruningCaptureFaultV1::walk(error)
                        })?
                        .1;
                    state = state
                        .try_delta_v1(&delta, params, revert)
                        .map_err(PalwStateWalkError::from)
                        .map_err(PalwPruningCaptureFaultV1::walk)?;
                    checkpoint()?;
                    completed += 1;
                    if progress_at.elapsed() >= std::time::Duration::from_secs(30) || completed == total {
                        info!(
                            "PALW: pruning snapshot {point} replay {completed}/{total} deltas, elapsed {} s",
                            started.elapsed().as_secs()
                        );
                        progress_at = std::time::Instant::now();
                    }
                    std::thread::yield_now();
                }
            }
        }
        if state.last_point().is_some_and(|last| last.block != point) || (state.last_point().is_none() && point != self.genesis.hash) {
            return Err(PalwPruningCaptureFaultV1::Context("reconstructed snapshot last point differs"));
        }
        checkpoint()?;
        // Serialize only publication with import/point changes. A busy writer
        // must not discard a long completed replay. Poll in bounded 10ms slices,
        // holding NO guards between attempts, checking cancellation/generation.
        let publication_started = std::time::Instant::now();
        let mut progress_at = publication_started;
        loop {
            if !keep_running() {
                return Err(PalwPruningCaptureFaultV1::Context("pruning capture interrupted before publication"));
            }
            let publish = || -> Result<(), PalwPruningCaptureFaultV1> {
                let _import = self
                    .pruning_import_session_lock
                    .try_lock()
                    .ok_or(PalwPruningCaptureFaultV1::Context("import mutation in progress"))?;
                let pruning =
                    self.pruning_point_store.try_read().ok_or(PalwPruningCaptureFaultV1::Context("pruning point is changing"))?;
                let meta =
                    self.pruning_meta_stores.try_read().ok_or(PalwPruningCaptureFaultV1::Context("import metadata is changing"))?;
                let session = meta.import_session().map_err(PalwPruningCaptureFaultV1::store)?;
                let fresh = PruningCaptureContextV1 {
                    pruning_point: pruning.pruning_point().map_err(PalwPruningCaptureFaultV1::store)?,
                    epoch: session.epoch,
                    active: session.active,
                    utxos_started: session.utxos_started,
                    utxos_imported: session.utxos_imported,
                    stable: meta.try_pruning_utxoset_stable_flag().map_err(PalwPruningCaptureFaultV1::store)?,
                };
                if fresh != context {
                    return Err(PalwPruningCaptureFaultV1::Context("pruning/import generation changed before publication"));
                }
                let mut store =
                    self.palw_state_v2_store.try_write().ok_or(PalwPruningCaptureFaultV1::Context("PALW publication is busy"))?;
                let mut batch = WriteBatch::default();
                let _ticket = store
                    .set_pruning_snapshot_batch_tracked_v1(&mut batch, point, &state)
                    .map_err(PalwPruningCaptureFaultV1::store)?;
                self.db.write(batch).map_err(|e| PalwPruningCaptureFaultV1::store(e.into()))?;
                Ok(())
            };
            match publish() {
                Ok(()) => break,
                Err(PalwPruningCaptureFaultV1::Context(
                    "import mutation in progress"
                    | "pruning point is changing"
                    | "import metadata is changing"
                    | "PALW publication is busy",
                )) => {
                    if progress_at.elapsed() >= std::time::Duration::from_secs(30) {
                        info!(
                            "PALW: pruning snapshot {point} awaiting publication for {} s; computed state retained",
                            publication_started.elapsed().as_secs()
                        );
                        progress_at = std::time::Instant::now();
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        }
        info!(
            "PALW: pruning snapshot {point} publication durable after {} ms ({} ms total)",
            publication_started.elapsed().as_millis(),
            started.elapsed().as_millis()
        );
        // Serving verification may decode/copy the carriage. It also belongs
        // outside the global PALW write lock; GC still waits for its success.
        let carriage = detached
            .load_pruning_snapshot_carriage_cached(params, point)
            .map_err(PalwPruningCaptureFaultV1::store)?
            .ok_or(PalwPruningCaptureFaultV1::Context("durable exact-point snapshot is not servable"))?;
        self.require_served_pruning_point_v1(point, &carriage)?;
        checkpoint()?;
        info!("PALW: captured and verified the V2 state at pruning point {point} in {} ms", started.elapsed().as_millis());
        Ok(())
    }

    /// Both ordinary advancement and cold recovery use this exact guard. The
    /// deletion callback cannot execute on a store, resource or context fault.
    pub(crate) fn with_pruning_snapshots_before_delete_v1<T>(
        &self,
        point: BlockHash,
        delete: impl FnOnce() -> T,
    ) -> Result<T, PalwPruningCaptureFaultV1> {
        self.with_pruning_snapshots_before_delete_while_v1(point, || true, delete)
    }

    pub(crate) fn with_pruning_snapshots_before_delete_while_v1<T>(
        &self,
        point: BlockHash,
        keep_running: impl Fn() -> bool,
        delete: impl FnOnce() -> T,
    ) -> Result<T, PalwPruningCaptureFaultV1> {
        if self.dns_params.is_some() {
            // Cold recovery may follow a partially completed deletion. Reuse
            // the existing serving row for exactly this point; a read fault is
            // not absence and must not trigger reconstruction from deleted rows.
            let existing = self.pruning_overlay_snapshot_store.read().get().optional().map_err(PalwPruningCaptureFaultV1::store)?;
            if existing.as_ref().is_none_or(|snapshot| snapshot.pruning_point != point) {
                self.capture_pruning_point_overlay_snapshot(point).map_err(|error| match error {
                    RuleError::LocalNativeCapacityUnavailable(capacity) => PalwPruningCaptureFaultV1::NativeResource(capacity),
                    other => PalwPruningCaptureFaultV1::Overlay(other),
                })?;
            }
        }
        self.capture_pruning_point_palw_state_while_v1(point, &keep_running)?;
        if !keep_running() {
            return Err(PalwPruningCaptureFaultV1::Context("pruning capture interrupted before deletion"));
        }
        Ok(delete())
    }
}
