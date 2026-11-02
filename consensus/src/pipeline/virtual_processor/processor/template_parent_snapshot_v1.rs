//! Node-local template memo; never used to weigh or validate a candidate.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1;
use kaspa_consensus_core::pruning::PruningImportSessionV1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParentKeyV1 {
    target: BlockHash,
    root: Option<kaspa_consensus_core::Hash64>,
    epoch: u64,
    pruning_point: BlockHash,
    active: Option<PruningImportSessionV1>,
    stable: bool,
}

pub(in crate::pipeline::virtual_processor) struct PreparedTemplateParentV1 {
    key: Option<ParentKeyV1>,
    pub(super) state: Option<(BlockHash, Arc<NativeChainStateV1>)>,
    pub(super) checked: Result<(), PalwWeighFaultV2>,
}

#[derive(Default)]
pub(in crate::pipeline::virtual_processor) struct TemplateParentSnapshotCacheV1 {
    // A single retained owner, per processor instance. No wall-clock TTL: a
    // block's authenticated post-state does not expire after one second.
    entry: parking_lot::Mutex<Option<(ParentKeyV1, Arc<NativeChainStateV1>)>>,
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) loads: std::sync::atomic::AtomicU64,
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) load_probe: parking_lot::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

// The single-flight lock is private to template readers. Neither virtual
// commits nor import writers take it, and load never holds their global guard.
fn memoized_parent_v1<T, E: Copy>(
    slot: &parking_lot::Mutex<Option<(ParentKeyV1, Arc<T>)>>,
    current: impl Fn() -> Result<ParentKeyV1, E>,
    load: impl FnOnce() -> Result<Arc<T>, E>,
    changed: E,
) -> Result<Arc<T>, E> {
    let mut entry = slot.lock();
    let key = match current() {
        Ok(key) => key,
        Err(error) => {
            *entry = None;
            return Err(error);
        }
    };
    if !key.stable || key.active.is_some() {
        *entry = None;
        return Err(changed);
    }
    if key.root.is_some()
        && let Some((old, state)) = entry.as_ref()
        && *old == key
    {
        let state = Arc::clone(state);
        match current() {
            Ok(fresh) if fresh == key => return Ok(state),
            other => {
                *entry = None;
                return Err(other.err().unwrap_or(changed));
            }
        }
    }
    // Release the previous reservation before requesting replacement capacity.
    // External consumers continue owning their own Arc/lease until they finish.
    *entry = None;
    let state = load()?;
    if current()? != key {
        return Err(changed);
    }
    // An absent compact identity is not a new refusal: serve the ordinary
    // checked reconstruction, but do not make an unverifiable cache entry.
    if key.root.is_some() {
        *entry = Some((key, Arc::clone(&state)));
    }
    Ok(state)
}

fn store_fault_v1(error: kaspa_database::prelude::StoreError) -> PalwWeighFaultV2 {
    match error {
        kaspa_database::prelude::StoreError::LocalResourceUnavailable(_) => PalwWeighFaultV2::NativeCapacityUnavailable,
        _ => PalwWeighFaultV2::StoreUnreadable,
    }
}

fn walk_fault_v1(error: crate::processes::palw_state_walk::PalwStateWalkError) -> PalwWeighFaultV2 {
    use crate::processes::palw_state_walk::PalwStateWalkError as W;
    if error.native_capacity_v1().is_some() {
        return PalwWeighFaultV2::NativeCapacityUnavailable;
    }
    if let Some(context) = error.native_context_v1() {
        use kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1 as F;
        return match context {
            F::Capacity(_) | F::CapacityUnavailable => PalwWeighFaultV2::NativeCapacityUnavailable,
            F::HistoryUnavailable => PalwWeighFaultV2::NoOpinion,
            F::StoreUnreadable => PalwWeighFaultV2::StoreUnreadable,
        };
    }
    match error {
        W::MissingDelta(_) | W::NoAnchor => PalwWeighFaultV2::NoOpinion,
        W::Store(_) | W::State(_) | W::Resource(_) => PalwWeighFaultV2::StoreUnreadable,
    }
}

impl VirtualStateProcessor {
    fn template_parent_key_v1(&self, target: BlockHash) -> Result<ParentKeyV1, PalwWeighFaultV2> {
        let meta = self.pruning_meta_stores.try_read().ok_or(PalwWeighFaultV2::NoOpinion)?;
        let import = meta.import_session().map_err(store_fault_v1)?;
        let stable = meta.try_pruning_utxoset_stable_flag().map_err(store_fault_v1)?;
        drop(meta);
        let store = self.palw_state_v2_store.try_read().ok_or(PalwWeighFaultV2::NoOpinion)?;
        // Match the original exact-tip reader: the root-verified materialized
        // tip is authoritative at its own block. A retained delta can predate
        // a freshly installed carriage at that same block.
        let root = match store.tip_key().map_err(store_fault_v1)? {
            Some((block, root)) if block == target => Some(root),
            _ => match store.state_root_of(target) {
                Ok(root) => Some(root),
                Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => {
                    let checkpoint = store.pruning_snapshot_key().map_err(store_fault_v1)?;
                    checkpoint.filter(|(block, _)| *block == target).map(|(_, root)| root)
                }
                Err(error) => return Err(store_fault_v1(error)),
            },
        };
        drop(store);
        let pruning_point =
            self.pruning_point_store.try_read().ok_or(PalwWeighFaultV2::NoOpinion)?.pruning_point().map_err(store_fault_v1)?;
        Ok(ParentKeyV1 { target, root, epoch: import.epoch, pruning_point, active: import.active, stable })
    }

    /// Same checked tip/delta walk as the existing candidate reader, exclusively
    /// for templates. The general fork-choice reader remains unchanged.
    pub(in crate::pipeline::virtual_processor) fn palw_template_parent_checked_v1(
        &self,
        target: BlockHash,
    ) -> Result<Arc<NativeChainStateV1>, PalwWeighFaultV2> {
        let params = self.palw_state_params_v2.as_ref().ok_or(PalwWeighFaultV2::NoOpinion)?;
        memoized_parent_v1(
            &self.template_parent_snapshot_v1.entry,
            || self.template_parent_key_v1(target),
            || {
                #[cfg(test)]
                self.template_parent_snapshot_v1.loads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // A template warmup may have seeded this SAME native owner
                // into the ordinary sink reader. Drop that memo before a cold
                // replacement asks for capacity; external readers retain theirs.
                *self.palw_reader_snapshot.lock() = None;
                // Clone only the store handle. Decode/replay must not hold its
                // global reader guard and block a virtual writer for seconds.
                let detached = self.palw_state_v2_store.try_read().ok_or(PalwWeighFaultV2::NoOpinion)?.clone();
                #[cfg(test)]
                if let Some(probe) = self.template_parent_snapshot_v1.load_probe.lock().take() {
                    probe();
                }
                let (tip, state) = detached.load_tip_cached(params).map_err(store_fault_v1)?.ok_or(PalwWeighFaultV2::NoOpinion)?;
                if !self.reachability_service.has_reachability_data(target) || !self.reachability_service.has_reachability_data(tip) {
                    return Err(PalwWeighFaultV2::NoOpinion);
                }
                let state = if tip == target {
                    state
                } else {
                    let path = self.dag_traversal_manager.calculate_chain_path(tip, target, None);
                    let state = state.try_clone_v1().map_err(|_| PalwWeighFaultV2::NativeCapacityUnavailable)?;
                    Arc::new(
                        crate::processes::palw_state_walk::walk_chain_path_retained_v1(
                            &detached,
                            params,
                            state,
                            &path.removed.to_vec(),
                            &path.added.to_vec(),
                        )
                        .map_err(walk_fault_v1)?,
                    )
                };
                if state.last_point().map(|point| point.block) != Some(target)
                    && !(target == self.genesis.hash && state.last_point().is_none())
                {
                    return Err(PalwWeighFaultV2::StoreUnreadable);
                }
                if let Some(root) = self.template_parent_key_v1(target)?.root
                    && state.state_root() != root
                {
                    return Err(PalwWeighFaultV2::StoreUnreadable);
                }
                Ok(state)
            },
            PalwWeighFaultV2::NoOpinion,
        )
    }

    /// Run every cold walk before the caller takes a virtual UTXO read guard.
    pub(in crate::pipeline::virtual_processor) fn prepare_template_parent_v1(
        &self,
        virtual_state: &VirtualState,
    ) -> Result<PreparedTemplateParentV1, PalwWeighFaultV2> {
        let target = virtual_state.ghostdag_data.selected_parent;
        // Bind BOTH loaders to the same captured generation. Reading the key
        // only after loading could relabel an older owner with a newer epoch.
        let key = self.palw_state_params_v2.as_ref().map(|_| self.template_parent_key_v1(target)).transpose()?;
        let checked = self.palw_template_parent_checked_v1(target);
        let state = match &checked {
            Ok(state) => Some((target, Arc::clone(state))),
            Err(_) => self.template_parent_legacy_fallback_v1(target),
        };
        if let (Some(key), Some((at, state))) = (key, state.as_ref())
            && (*at != key.target || (checked.is_ok() && key.root.is_some_and(|root| state.state_root() != root)))
        {
            return Err(PalwWeighFaultV2::StoreUnreadable);
        }
        let prepared = PreparedTemplateParentV1 { key, state, checked: checked.map(|_| ()) };
        self.check_prepared_template_parent_v1(&prepared)?;
        Ok(prepared)
    }

    /// Only fixed-size metadata reads. This method MUST NOT run a cold loader.
    pub(super) fn check_prepared_template_parent_v1(&self, prepared: &PreparedTemplateParentV1) -> Result<(), PalwWeighFaultV2> {
        if let Some(key) = prepared.key
            && self.template_parent_key_v1(key.target)? != key
        {
            return Err(PalwWeighFaultV2::NoOpinion);
        }
        Ok(())
    }

    /// Populate the existing sink-reader memo only from the strict exact
    /// ACTUAL sink. Carrier checks under the subsequent UTXO guard must hit this
    /// shared owner rather than perform another cold reconstruction.
    pub(in crate::pipeline::virtual_processor) fn seed_template_sink_reader_v1(
        &self,
        captured: &Arc<VirtualState>,
        prepared: &PreparedTemplateParentV1,
    ) -> Result<(), PalwWeighFaultV2> {
        if self.palw_state_params_v2.is_none() {
            return Ok(());
        }
        // This is an optimization, never another template admission gate.
        // Legacy pruning fallback and absent compact metadata remain usable
        // through their original slow paths. They cannot seed a strict memo.
        if prepared.checked.is_err() {
            return Ok(());
        }
        let Some((at, state)) = prepared.state.as_ref() else { return Ok(()) };
        let Some(key) = prepared.key else { return Ok(()) };
        if !key.stable || key.active.is_some() {
            return Ok(());
        }
        let Some(root) = key.root else { return Ok(()) };
        if *at != captured.ghostdag_data.selected_parent
            || key.target != *at
            || (state.last_point().map(|p| p.block) != Some(*at) && !(*at == self.genesis.hash && state.last_point().is_none()))
            || state.state_root() != root
        {
            return Err(PalwWeighFaultV2::StoreUnreadable);
        }
        let fresh = || {
            let actual = self.virtual_stores.try_read().ok_or(PalwWeighFaultV2::NoOpinion)?.state.get().map_err(store_fault_v1)?;
            if !Arc::ptr_eq(captured, &actual) || !Arc::ptr_eq(captured, &self.lkg_virtual_state.load_full()) {
                return Err(PalwWeighFaultV2::NoOpinion);
            }
            self.check_prepared_template_parent_v1(prepared)
        };
        fresh()?;
        let mut memo = self.palw_reader_snapshot.lock();
        fresh()?;
        *memo = Some((*at, Arc::clone(state)));
        if let Err(error) = fresh() {
            *memo = None;
            return Err(error);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn old_template_parent_for_tests_v1(
        &self,
        target: BlockHash,
    ) -> Option<(BlockHash, Arc<NativeChainStateV1>)> {
        self.palw_v2_state_at(target)
    }

    /// Preserve the legacy pruning-snapshot fallback using a detached store.
    /// Its result never satisfies strict source-bootstrap candidate preflight.
    fn template_parent_legacy_fallback_v1(
        &self,
        at: BlockHash,
    ) -> Option<(BlockHash, Arc<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1>)> {
        let params = self.palw_state_params_v2.as_ref()?;
        let before = self.template_parent_key_v1(at).ok()?;
        // An incomplete sidecar import disables the strict memo. It was not a
        // legacy template admission gate; preserve the old checked fallback
        // while still refusing a generation change across this cold read.
        let store = self.palw_state_v2_store.try_read()?.clone();
        let (tip_block, tip_state) = store.load_tip_cached(params).ok().flatten()?;
        if tip_block == at {
            return (self.template_parent_key_v1(at).ok()? == before).then_some((tip_block, tip_state));
        }
        if !self.reachability_service.has_reachability_data(tip_block) || !self.reachability_service.has_reachability_data(at) {
            return None;
        }
        let path = self.dag_traversal_manager.calculate_chain_path(tip_block, at, None);
        let removed: Vec<BlockHash> = path.removed.to_vec();
        let added: Vec<BlockHash> = path.added.to_vec();
        match crate::processes::palw_state_walk::walk_chain_path_retained_v1(
            &store,
            params,
            tip_state.try_clone_v1().ok()?,
            &removed,
            &added,
        ) {
            Ok(state) => {
                warn!(
                    "PALW V2 state tip stood at {tip_block} while the template selected parent is {at}; re-derived the parent state over {} reverted and {} applied deltas so the committed root matches the validating walk",
                    removed.len(),
                    added.len()
                );
                (self.template_parent_key_v1(at).ok()? == before).then(|| (at, Arc::new(state)))
            }
            Err(e) => match store.load_pruning_snapshot_retained_v1(params) {
                Ok(Some((snap_block, snap_state))) => {
                    let snap_path = self.dag_traversal_manager.calculate_chain_path(snap_block, at, None);
                    let snap_removed: Vec<BlockHash> = snap_path.removed.to_vec();
                    let snap_added: Vec<BlockHash> = snap_path.added.to_vec();
                    match crate::processes::palw_state_walk::walk_chain_path_retained_v1(
                        &store,
                        params,
                        snap_state,
                        &snap_removed,
                        &snap_added,
                    ) {
                        Ok(state) => {
                            warn!(
                                "PALW V2 state tip stands at {tip_block}, template selected parent is {at}, and the path between them cannot be walked ({e}); rebuilt the parent state from the pruning snapshot at {snap_block}"
                            );
                            (self.template_parent_key_v1(at).ok()? == before).then(|| (at, Arc::new(state)))
                        }
                        Err(snap_err) => {
                            warn!(
                                "PALW V2 template cannot establish the state at selected parent {at}: tip {tip_block} does not walk here ({e}) and neither does the pruning snapshot at {snap_block} ({snap_err}); leaving palw_state_root unset rather than committing the store tip"
                            );
                            None
                        }
                    }
                }
                Ok(None) => {
                    warn!(
                        "PALW V2 template cannot establish the state at selected parent {at}: tip {tip_block} does not walk here ({e}) and this store holds no pruning snapshot; leaving palw_state_root unset rather than committing the store tip"
                    );
                    None
                }
                Err(snap_err) => {
                    warn!(
                        "PALW V2 template cannot establish the state at selected parent {at}: tip {tip_block} does not walk here ({e}) and the pruning snapshot cannot be read ({snap_err}); leaving palw_state_root unset rather than committing the store tip"
                    );
                    None
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn key() -> ParentKeyV1 {
        ParentKeyV1 {
            target: BlockHash::from_u64_word(1),
            root: Some(kaspa_consensus_core::Hash64::from_u64_word(2)),
            epoch: 3,
            pruning_point: BlockHash::from_u64_word(0),
            active: None,
            stable: true,
        }
    }

    #[test]
    fn template_parent_memo_identity_epoch_import_errors_and_missing_metadata() {
        let slot = parking_lot::Mutex::new(None);
        let loads = AtomicU64::new(0);
        let get = |key, failure| {
            memoized_parent_v1(
                &slot,
                || Ok(key),
                || {
                    loads.fetch_add(1, Ordering::SeqCst);
                    if failure { Err(1) } else { Ok(Arc::new(7)) }
                },
                2,
            )
        };
        let first = get(key(), false).unwrap();
        assert!(Arc::ptr_eq(&first, &get(key(), false).unwrap()));
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        let mut changed = key();
        changed.root = Some(kaspa_consensus_core::Hash64::from_u64_word(4));
        get(changed, false).unwrap();
        changed.epoch += 1;
        get(changed, false).unwrap();
        changed.target = BlockHash::from_u64_word(5);
        assert!(get(changed, true).is_err());
        assert!(get(changed, true).is_err(), "a failed load is never memoized");
        assert!(slot.lock().is_none());
        assert_eq!(loads.load(Ordering::SeqCst), 5);
        changed.active = Some(PruningImportSessionV1 { point: changed.target, epoch: changed.epoch });
        assert_eq!(get(changed, false), Err(2));
        changed.active = None;
        changed.stable = false;
        assert_eq!(get(changed, false), Err(2));
        changed.stable = true;
        changed.root = None;
        get(changed, false).unwrap();
        get(changed, false).unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 7, "missing metadata serves the checked loader without caching");
    }

    #[test]
    fn template_parent_memo_releases_old_owner_before_load_and_detects_generation_change() {
        let slot = parking_lot::Mutex::new(Some((key(), Arc::new(7))));
        let weak = Arc::downgrade(&slot.lock().as_ref().unwrap().1);
        let mut changed = key();
        changed.epoch += 1;
        let calls = AtomicU64::new(0);
        let result = memoized_parent_v1(
            &slot,
            || {
                let mut key = changed;
                if calls.fetch_add(1, Ordering::SeqCst) > 0 {
                    key.epoch += 1;
                }
                Ok(key)
            },
            || {
                assert!(weak.upgrade().is_none(), "drop the old reservation before allocating replacement");
                Ok(Arc::new(8))
            },
            1,
        );
        assert_eq!(result, Err(1));
        assert!(slot.lock().is_none());
    }

    #[test]
    fn template_parent_memo_single_flight_and_external_owner_lifetime() {
        let slot = Arc::new(parking_lot::Mutex::new(None));
        let loads = Arc::new(AtomicU64::new(0));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let slot = Arc::clone(&slot);
                let loads = Arc::clone(&loads);
                std::thread::spawn(move || {
                    memoized_parent_v1(
                        &slot,
                        || Ok(key()),
                        || {
                            loads.fetch_add(1, Ordering::SeqCst);
                            Ok(Arc::new(7))
                        },
                        1,
                    )
                    .unwrap()
                })
            })
            .collect();
        let external = threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>();
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert!(external.iter().all(|state| Arc::ptr_eq(state, &external[0])));
        let weak = Arc::downgrade(&external[0]);
        *slot.lock() = None;
        assert!(weak.upgrade().is_some(), "the consumer keeps the retained state alive after cache invalidation");
        drop(external);
        assert!(weak.upgrade().is_none());
    }
}
