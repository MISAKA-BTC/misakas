//! A writer for a table whose only dependent state is its delta entry.
//!
//! This scope borrows one map, the journal, and the existing native construction
//! owner. It cannot change another table, resolve rules, or publish a state.
//! Admission precedes the map clone and the journal push, and the reservation
//! remains with the transition after this short-lived borrow ends.
//!
//! Tables with derived indexes or cascading writes keep their dedicated writers.
//! In particular, claims, bonds, courts, and vesting must not use this boundary
//! to bypass the associated accounting or index maintenance.

use super::native_writers::NativeWriteQuoteV1;
use super::{PalwDeltaEntryV2, PalwStateV2Error};
use crate::palw_native_resource_v1::{NativeHeapV1, NativeTransitionReservationV1};
use std::collections::BTreeMap;

pub(super) struct JournaledMapWriterV1<'a, K, V> {
    rows: &'a mut BTreeMap<K, V>,
    entries: &'a mut Vec<PalwDeltaEntryV2>,
    reservation: &'a mut NativeTransitionReservationV1,
    path: &'static str,
}

impl<'a, K, V> JournaledMapWriterV1<'a, K, V>
where
    K: Copy + Ord + NativeHeapV1,
    V: Clone + NativeHeapV1,
{
    pub(super) fn new(
        rows: &'a mut BTreeMap<K, V>,
        entries: &'a mut Vec<PalwDeltaEntryV2>,
        reservation: &'a mut NativeTransitionReservationV1,
        path: &'static str,
    ) -> Self {
        Self { rows, entries, reservation, path }
    }

    fn reserve(&mut self, key: &K, new: &Option<V>) -> Result<(), PalwStateV2Error> {
        if !self.reservation.is_tracking_v1() {
            return Ok(());
        }
        NativeWriteQuoteV1::map_row(key, new, self.rows, false, self.path)?.admit(self.reservation, self.entries)
    }

    fn replace(&mut self, key: K, new: &Option<V>) -> Option<V> {
        match new {
            Some(row) => self.rows.insert(key, row.clone()),
            None => self.rows.remove(&key),
        }
    }

    /// Preserve writers that journal every call, including equal replacement
    /// and removal of an absent row. These entries are part of delta history.
    pub(super) fn write(
        mut self,
        key: K,
        new: Option<V>,
        entry: fn(K, Option<V>, Option<V>) -> PalwDeltaEntryV2,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve(&key, &new)?;
        let old = self.replace(key, &new);
        self.entries.push(entry(key, old, new));
        Ok(())
    }

    /// Preserve writers that replace the row on every call and only journal a
    /// changed value. Admission still precedes replacement, even for a no-op.
    pub(super) fn write_if_changed(
        mut self,
        key: K,
        new: Option<V>,
        entry: fn(K, Option<V>, Option<V>) -> PalwDeltaEntryV2,
    ) -> Result<(), PalwStateV2Error>
    where
        V: PartialEq,
    {
        self.reserve(&key, &new)?;
        let old = self.replace(key, &new);
        if old != new {
            self.entries.push(entry(key, old, new));
        }
        Ok(())
    }
}
