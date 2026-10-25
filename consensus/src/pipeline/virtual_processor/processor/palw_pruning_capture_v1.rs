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

impl VirtualStateProcessor {
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
        let Some(params) = self.palw_state_params_v2.as_ref() else { return Ok(()) };
        let store = self.palw_state_v2_store.read();
        // A prior capture may already be durable when recovery resumes a partial
        // prune. Validate and reuse it without requiring already-deleted history.
        if let Some(carriage) = store.load_pruning_snapshot_carriage_cached(params, point).map_err(PalwPruningCaptureFaultV1::store)? {
            return self.require_served_pruning_point_v1(point, &carriage);
        }
        let (tip, tip_state) = store
            .load_tip_retained_v1(params)
            .map_err(PalwPruningCaptureFaultV1::store)?
            .ok_or(PalwPruningCaptureFaultV1::Context("active PALW tip is absent"))?;
        let state = if tip == point {
            tip_state
        } else {
            let path = self.dag_traversal_manager.calculate_chain_path(tip, point, None);
            crate::processes::palw_state_walk::walk_chain_path_retained_v1(
                &store,
                params,
                tip_state,
                &path.removed.to_vec(),
                &path.added.to_vec(),
            )
            .map_err(PalwPruningCaptureFaultV1::walk)?
        };
        drop(store);
        let mut batch = WriteBatch::default();
        let mut store = self.palw_state_v2_store.write();
        let _ticket =
            store.set_pruning_snapshot_batch_tracked_v1(&mut batch, point, &state).map_err(PalwPruningCaptureFaultV1::store)?;
        // Keep the same exclusion/ticket until this exact snapshot is durable.
        self.db.write(batch).map_err(|e| PalwPruningCaptureFaultV1::store(e.into()))?;
        let carriage = store
            .load_pruning_snapshot_carriage_cached(params, point)
            .map_err(PalwPruningCaptureFaultV1::store)?
            .ok_or(PalwPruningCaptureFaultV1::Context("durable exact-point snapshot is not servable"))?;
        self.require_served_pruning_point_v1(point, &carriage)?;
        info!("PALW: captured and verified the V2 state at pruning point {point}");
        Ok(())
    }

    /// Both ordinary advancement and cold recovery use this exact guard. The
    /// deletion callback cannot execute on a store, resource or context fault.
    pub(crate) fn with_pruning_snapshots_before_delete_v1<T>(
        &self,
        point: BlockHash,
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
        self.capture_pruning_point_palw_state(point)?;
        Ok(delete())
    }
}
