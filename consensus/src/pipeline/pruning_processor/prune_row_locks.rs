//! Shared production order for one atomic pruning row batch. Virtual commits
//! retain PALW archive exclusion until their status and delta batch is durable.
use crate::{
    consensus::storage::ConsensusStorage,
    model::stores::{palw_state_v2::DbPalwStateV2Store, relations::DbRelationsStore, statuses::DbStatusesStore},
};
use parking_lot::RwLockWriteGuard;

pub(super) struct RowGuards<'a> {
    pub palw: RwLockWriteGuard<'a, DbPalwStateV2Store>,
    pub relations: RwLockWriteGuard<'a, DbRelationsStore>,
    pub reachability_relations: RwLockWriteGuard<'a, DbRelationsStore>,
    pub statuses: RwLockWriteGuard<'a, DbStatusesStore>,
}

pub(super) fn lock_rows(storage: &ConsensusStorage) -> RowGuards<'_> {
    lock_rows_after(storage, || {})
}

// The no-op production call and deterministic test share the actual four-store
// acquisition sequence. The test barrier is before PALW, never between a held
// statuses guard and PALW. No callback supplies a guard or changes this order.
fn lock_rows_after(storage: &ConsensusStorage, before_palw: impl FnOnce()) -> RowGuards<'_> {
    before_palw();
    let palw = storage.palw_state_v2_store.write();
    let relations = storage.relations_store.write();
    let reachability_relations = storage.reachability_relations_store.write();
    let statuses = storage.statuses_store.write();
    RowGuards { palw, relations, reachability_relations, statuses }
}

#[cfg(test)]
#[path = "prune_row_locks_tests.rs"]
mod tests;
