//! Own-node source read session. Every operation checks native publication,
//! import generation, rooted Case prefix and actual unspent funding again.
use super::VirtualStateProcessor;
use crate::model::stores::utxo_set::UtxoSetStoreReader;
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_positive_rows_v2::PositiveRowsV2,
    palw_positive_source_read_v2::{
        PositiveSourceReadBoundsV2, PositiveSourceReadErrorV2 as E, PositiveSourceReadV2, PositiveSourceUnitBoundsV2,
        PositiveSourceViewIdV2,
    },
    palw_state_v2::{
        NativeAssignedCaseBackingV2, NativeAssignedSourceSnapshotV2, PalwStateV2Error, replay_assigned_source_snapshot_v2,
    },
};
use kaspa_database::prelude::StoreResultExt;
use misaka_palw_positive_contract::{
    identity::{RequestId, ResultId},
    messages::{MinerDataStatus, MinerResultCommit, SourceRequest},
    native::NativeRowKey,
    rows::NativeRowBody,
};
use std::sync::{Arc, atomic::Ordering};

struct NativePositiveSourceReadV2 {
    owner: Arc<VirtualStateProcessor>,
    view: PositiveSourceViewIdV2,
    import_epoch: u64,
    snapshot: NativeAssignedSourceSnapshotV2,
    _lease: NativeDemandLeaseV1,
}
impl VirtualStateProcessor {
    pub(crate) fn positive_source_duty_v2(
        self: &Arc<Self>,
        request: RequestId,
        result: ResultId,
    ) -> Result<Box<dyn PositiveSourceReadV2>, E> {
        let (generation, virtual_state) = self.lkg_virtual_state.load_versioned();
        let now = virtual_state.daa_score;
        if !self.palw_lifecycle_kind_fences.positive_court_reserved_active_at_v2(now) {
            return Err(E::Inactive);
        }
        let import_epoch = self.positive_source_import_epoch_v2()?;
        let lease = reserve_native_requests_v1(size_of::<NativePositiveSourceReadV2>() as u64)
            .map_err(|_| E::Unavailable)?
            .ok_or(E::Unavailable)?;
        let params = self.palw_state_params_v2.as_ref().ok_or(E::Inactive)?;
        let (parent, state) = self.palw_v2_reader_state(params).map_err(|_| E::Unavailable)?.ok_or(E::Unavailable)?;
        if parent != virtual_state.ghostdag_data.selected_parent {
            return Err(E::StaleView);
        }
        let row = PositiveRowsV2::new(&state).read(NativeRowKey::Result(result)).map_err(|_| E::Binding)?.ok_or(E::Missing)?;
        let NativeRowBody::Result(row) = row.body else { return Err(E::Binding) };
        if row.result.id() != result {
            return Err(E::Binding);
        }
        let backing = self
            .prepare_committed_case_history_v2(parent, &state, (row.result.case_id(), Some(result)))
            .map_err(|_| E::Unavailable)?
            .ok_or(E::Missing)?;
        let NativeAssignedCaseBackingV2::Committed(backing) = backing else { return Err(E::Missing) };
        let snapshot = replay_assigned_source_snapshot_v2(&state, &backing, request, result, now).map_err(|error| match error {
            PalwStateV2Error::NativeResource(_) | PalwStateV2Error::NativeContextUnavailable(_) => E::Unavailable,
            _ => E::Binding,
        })?;
        let read = NativePositiveSourceReadV2 {
            owner: self.clone(),
            import_epoch,
            snapshot,
            _lease: lease,
            view: PositiveSourceViewIdV2 {
                selected_parent: parent,
                state_root: state.state_root(),
                accepted_prefix: Hash64::from_bytes(backing.pin().head.head.0),
                generation,
            },
        };
        read.check_current()?;
        Ok(Box::new(read))
    }

    fn positive_source_import_epoch_v2(&self) -> Result<u64, E> {
        if self.is_consensus_exiting.load(Ordering::Acquire) {
            return Err(E::Unavailable);
        }
        let (session, stable) = self.pruning_import_status().map_err(|_| E::Unavailable)?;
        if !stable || session.active.is_some() {
            return Err(E::Unavailable);
        }
        Ok(session.epoch)
    }
}
impl NativePositiveSourceReadV2 {
    fn check_current(&self) -> Result<(), E> {
        let owner = &self.owner;
        let (generation, virtual_state) = owner.lkg_virtual_state.load_versioned();
        if generation != self.view.generation || virtual_state.ghostdag_data.selected_parent != self.view.selected_parent {
            return Err(E::StaleView);
        }
        if virtual_state.daa_score >= self.snapshot.bounds().cutoff {
            return Err(E::Deadline);
        }
        if owner.positive_source_import_epoch_v2()? != self.import_epoch {
            return Err(E::StaleView);
        }
        let params = owner.palw_state_params_v2.as_ref().ok_or(E::Inactive)?;
        let (parent, state) = owner.palw_v2_reader_state(params).map_err(|_| E::Unavailable)?.ok_or(E::Unavailable)?;
        if parent != self.view.selected_parent || state.state_root() != self.view.state_root {
            return Err(E::StaleView);
        }
        let (_, pin) = state
            .assigned_case_journal_pin_v2(self.snapshot.request().case, Some(self.snapshot.result().id()))
            .map_err(|_| E::Unavailable)?
            .ok_or(E::Missing)?;
        if pin.head.head.0 != self.view.accepted_prefix.as_bytes() {
            return Err(E::StaleView);
        }
        let entry = match self.snapshot.funding_outpoint() {
            Some(point) => owner.virtual_stores.read().utxo_set.get(&point).optional().map_err(|_| E::Unavailable)?,
            None => None,
        };
        if !self.snapshot.funding_utxo_matches(entry.as_deref()) {
            return Err(E::Unavailable);
        }
        // Recheck after all archive/UTXO reads, including an away-and-back move.
        if owner.lkg_virtual_state.load_versioned().0 != generation || owner.positive_source_import_epoch_v2()? != self.import_epoch {
            return Err(E::StaleView);
        }
        Ok(())
    }
}
impl PositiveSourceReadV2 for NativePositiveSourceReadV2 {
    fn request(&self) -> &SourceRequest {
        self.snapshot.request()
    }
    fn view_id(&self) -> PositiveSourceViewIdV2 {
        self.view
    }
    fn authorize(&self, result: &MinerResultCommit) -> Result<PositiveSourceReadBoundsV2, E> {
        self.check_current()?;
        if result != self.snapshot.result() {
            return Err(E::Binding);
        }
        Ok(self.snapshot.bounds())
    }
    fn unit_bounds(&self, unit: u64) -> Result<PositiveSourceUnitBoundsV2, E> {
        self.check_current()?;
        self.snapshot.unit(unit).map(|(bound, _)| bound)
    }
    fn status(&self, unit: u64) -> Result<MinerDataStatus, E> {
        self.check_current()?;
        self.snapshot.unit(unit).map(|(_, status)| status)
    }
}
