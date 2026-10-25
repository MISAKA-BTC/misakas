//! Exact post-target native state; no public RPC consumer yet.
//! This entry owns a real reader-first pruning session. Import remains stable
//! throughout; no source failure selects the current/latest state.
use super::*;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::processes::palw_state_walk::PalwStateWalkError;
use kaspa_consensus_core::palw_native_resource_v1::{NativeChainStateV1, NativeCoverageV1, reserve_native_requests_v1};
use kaspa_database::prelude::StoreError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum NativeEvmAsOfFaultV2 {
    #[error("historical PALW local capacity: {0}")]
    Resource(NativeCoverageV1),
    #[error("historical PALW history unavailable")]
    HistoryUnavailable,
    #[error("historical PALW local capacity unavailable without a numeric quote")]
    CapacityUnavailable,
    #[error("historical PALW native context: {0}")]
    NativeContext(kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1),
    #[error("historical PALW store read: {0}")]
    Store(StoreError),
    #[error("historical PALW transition: {0}")]
    Walk(PalwStateWalkError),
    #[error("historical PALW reachability: {0}")]
    Reachability(crate::processes::reachability::ReachabilityError),
    #[error("historical PALW read context: {0}")]
    Context(&'static str),
}
impl NativeEvmAsOfFaultV2 {
    fn store(error: StoreError) -> Self {
        match error {
            StoreError::LocalResourceUnavailable(reason) => {
                Self::Resource(NativeCoverageV1::unsupported::<NativeChainStateV1>("cold historical native state", &reason))
            }
            StoreError::KeyNotFound(_) => Self::HistoryUnavailable,
            other => Self::Store(other),
        }
    }
    fn reachability(error: crate::processes::reachability::ReachabilityError) -> Self {
        match error {
            crate::processes::reachability::ReachabilityError::StoreError(error) => Self::store(error),
            other => Self::Reachability(other),
        }
    }
    fn walk(error: PalwStateWalkError) -> Self {
        if let Some(capacity) = error.native_capacity_v1() {
            return Self::Resource(capacity);
        }
        if let Some(context) = error.native_context_v1() {
            use kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1 as C;
            return match context {
                C::Capacity(coverage) => Self::Resource(coverage),
                C::CapacityUnavailable => Self::CapacityUnavailable,
                C::HistoryUnavailable => Self::HistoryUnavailable,
                other => Self::NativeContext(other),
            };
        }
        match error {
            PalwStateWalkError::NoAnchor | PalwStateWalkError::MissingDelta(_) => Self::HistoryUnavailable,
            other => Self::Walk(other),
        }
    }
}
/// Brent cycle detection retains one hash and scalar counters; equal DAA or
/// blue-work values are legitimate and are not used as an artificial deadline.
struct SelectedParentCycleV2 {
    anchor: BlockHash,
    power: usize,
    steps: usize,
}
impl SelectedParentCycleV2 {
    fn new(start: BlockHash) -> Self {
        Self { anchor: start, power: 1, steps: 0 }
    }
    fn observe(&mut self, next: BlockHash) -> Result<(), NativeEvmAsOfFaultV2> {
        if next == self.anchor {
            return Err(NativeEvmAsOfFaultV2::Context("cyclic selected parent"));
        }
        self.steps += 1;
        if self.steps == self.power {
            self.anchor = next;
            self.steps = 0;
            self.power = self
                .power
                .checked_mul(2)
                .ok_or_else(|| NativeEvmAsOfFaultV2::Resource(NativeCoverageV1::overflow::<Self>("selected-parent cycle counter")))?;
        }
        Ok(())
    }
}
impl VirtualStateProcessor {
    fn evm_asof_import_status_v2(
        &self,
    ) -> Result<(kaspa_consensus_core::pruning::PruningImportSessionStateV1, bool), NativeEvmAsOfFaultV2> {
        // The legacy status wrapper stringifies StoreError. Preserve the actual
        // source causes here before local Resource/history classification.
        let meta = self.pruning_meta_stores.read();
        let state = meta.import_session().map_err(NativeEvmAsOfFaultV2::store)?;
        let stable = meta.try_pruning_utxoset_stable_flag().map_err(NativeEvmAsOfFaultV2::store)?;
        Ok((state, stable))
    }
    #[cfg(test)]
    pub(crate) fn observe_evm_asof_before_recheck_v2(&self, callback: Option<fn(&VirtualStateProcessor)>) {
        BEFORE_RECHECK.with(|slot| slot.set(callback));
    }
    /// Exactly AFTER target. The target header commits its parent's PALW state,
    /// so it must not be used as this post-target state's expected root.
    pub(crate) fn palw_evm_post_state_owned_v2(&self, target: BlockHash) -> Result<Option<NativeChainStateV1>, NativeEvmAsOfFaultV2> {
        let Some(params) = self.palw_state_params_v2.as_ref() else { return Ok(None) };
        // Own one actual pruning session. The existing reader-first SessionLock
        // also permits callers that already hold a Consensus session to delegate.
        let _pruning = self.pruning_lock.blocking_read();
        let (generation, stable) = self.evm_asof_import_status_v2()?;
        if !stable || generation.active.is_some() {
            return Err(NativeEvmAsOfFaultV2::Context("import incomplete"));
        }
        let store = self.palw_state_v2_store.read();
        let (mut at, mut state) = store
            .load_tip_retained_v1(params)
            .map_err(NativeEvmAsOfFaultV2::store)?
            .ok_or(NativeEvmAsOfFaultV2::HistoryUnavailable)?;
        // Same state walk as calculate_chain_path, with no unreserved removed
        // vector. Each reverse transition already owns its state/delta tickets.
        let mut reverse_cycle = SelectedParentCycleV2::new(at);
        while !self.reachability_service.try_is_chain_ancestor_of(at, target).map_err(NativeEvmAsOfFaultV2::reachability)? {
            let parent = self.ghostdag_store.get_selected_parent(at).map_err(NativeEvmAsOfFaultV2::store)?;
            reverse_cycle.observe(parent)?;
            let delta = store.delta_of_retained_v1(at).map_err(NativeEvmAsOfFaultV2::store)?.1;
            state = state.try_delta_v1(&delta, params, true).map_err(|e| NativeEvmAsOfFaultV2::walk(e.into()))?;
            at = parent;
        }
        // Count immutable selected parents before allocating the forward path.
        // Compact ghostdag rows, not a serialized-state ratio, determine count.
        let mut count = 0usize;
        let mut cursor = target;
        let mut forward_cycle = SelectedParentCycleV2::new(target);
        while cursor != at {
            let parent = self.ghostdag_store.get_selected_parent(cursor).map_err(NativeEvmAsOfFaultV2::store)?;
            forward_cycle.observe(parent)?;
            count = count.checked_add(1).ok_or_else(|| {
                NativeEvmAsOfFaultV2::Resource(NativeCoverageV1::overflow::<Vec<BlockHash>>("historical native forward path"))
            })?;
            cursor = parent;
        }
        let path_bytes = (count as u64).checked_mul(std::mem::size_of::<BlockHash>() as u64).ok_or_else(|| {
            NativeEvmAsOfFaultV2::Resource(NativeCoverageV1::overflow::<Vec<BlockHash>>("historical native forward path"))
        })?;
        let _path_owner = reserve_native_requests_v1(path_bytes).map_err(NativeEvmAsOfFaultV2::Resource)?;
        let mut added = Vec::with_capacity(count);
        cursor = target;
        while cursor != at {
            if added.len() == count {
                return Err(NativeEvmAsOfFaultV2::Context("selected-parent path changed"));
            }
            added.push(cursor);
            cursor = self.ghostdag_store.get_selected_parent(cursor).map_err(NativeEvmAsOfFaultV2::store)?;
        }
        if added.len() != count {
            return Err(NativeEvmAsOfFaultV2::Context("selected-parent path changed"));
        }
        for block in added.into_iter().rev() {
            let delta = store.delta_of_retained_v1(block).map_err(NativeEvmAsOfFaultV2::store)?.1;
            state = state.try_delta_v1(&delta, params, false).map_err(|e| NativeEvmAsOfFaultV2::walk(e.into()))?;
        }
        if !state.last_point().is_some_and(|point| point.block == target)
            && !(target == self.genesis.hash && state.last_point().is_none())
        {
            return Err(NativeEvmAsOfFaultV2::Context("post-target last point differs"));
        }
        #[cfg(test)]
        BEFORE_RECHECK.with(|slot| {
            if let Some(callback) = slot.take() {
                callback(self);
            }
        });
        let (after, stable_after) = self.evm_asof_import_status_v2()?;
        if !stable_after || after.active.is_some() || after.epoch != generation.epoch {
            return Err(NativeEvmAsOfFaultV2::Context("import generation changed"));
        }
        Ok(Some(state))
    }
}

#[cfg(test)]
thread_local! { static BEFORE_RECHECK: std::cell::Cell<Option<fn(&VirtualStateProcessor)>>=const{std::cell::Cell::new(None)}; }
#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u8) -> BlockHash {
        BlockHash::from_bytes([n; 64])
    }
    #[test]
    fn evm_asof_draft_cycle_guard_detects_self_two_and_long_cycle() {
        for sequence in [vec![0], vec![1, 0, 1], vec![1, 2, 3, 1, 2, 3, 1, 2, 3]] {
            let mut guard = SelectedParentCycleV2::new(h(0));
            assert!(sequence.into_iter().any(|n| guard.observe(h(n)).is_err()));
        }
        let mut guard = SelectedParentCycleV2::new(h(0));
        for n in 1..100 {
            guard.observe(h(n)).unwrap();
        }
    }
    #[test]
    fn evm_asof_draft_reachability_preserves_actual_store_class() {
        use crate::processes::reachability::ReachabilityError as R;
        let missing = kaspa_database::prelude::DbKey::prefix_only(&[1]);
        assert!(matches!(
            NativeEvmAsOfFaultV2::reachability(R::StoreError(StoreError::KeyNotFound(missing))),
            NativeEvmAsOfFaultV2::HistoryUnavailable
        ));
        assert!(matches!(
            NativeEvmAsOfFaultV2::reachability(R::StoreError(StoreError::LocalResourceUnavailable("quota".into()))),
            NativeEvmAsOfFaultV2::Resource(_)
        ));
        assert!(matches!(
            NativeEvmAsOfFaultV2::reachability(R::DataInconsistency),
            NativeEvmAsOfFaultV2::Reachability(R::DataInconsistency)
        ));
    }
}
