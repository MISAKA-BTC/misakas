//! **Storage reservations** — the minimal interface the IBD lane needs from the node's capacity manager.
//!
//! G14 storage (2026-10-22): before a large receive, unpack or DB switch the node estimates the bytes it is about to
//! write and RESERVES them, so two concurrent tasks cannot both count the same free space. The capacity lane
//! (`storage/capacity`) owns the real manager; at the time this was written that branch had published no reservation
//! API, so this file defines the smallest trait the IBD/generation code calls. The capacity manager implements
//! [`StorageReserver`]; everything here works against `Arc<dyn StorageReserver>`.
//!
//! Default-safe: [`UnboundedReserver`] grants everything and is what a node gets when nothing is configured — i.e.
//! exactly today's behaviour. [`FixedBudgetReserver`] is the reference implementation (and what the tests use to run a
//! scaled-down cap).

use parking_lot::Mutex;
use std::{fmt, sync::Arc};

/// What a reservation is for. Node-local bookkeeping only; nothing here is consensus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StoragePurpose {
    /// A staging generation (the switch DB of a pruned IBD, or any other generation being built).
    StagingGeneration,
    /// One IBD receive/unpack spill file (capped, see `spill`).
    IbdSpill,
    /// Anything else the caller names.
    Other(&'static str),
}

impl fmt::Display for StoragePurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StagingGeneration => f.write_str("staging generation"),
            Self::IbdSpill => f.write_str("IBD spill"),
            Self::Other(what) => f.write_str(what),
        }
    }
}

/// A refused reservation. Capacity shortage is a LOCAL condition: callers must turn it into "pause / retry", never into
/// a verdict on a peer or a block.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("storage reservation refused for {purpose}: requested {requested} bytes, {available} available")]
pub struct ReserveRefused {
    pub purpose: StoragePurpose,
    pub requested: u64,
    pub available: u64,
}

/// The capacity manager's side. `try_reserve` must be atomic with respect to concurrent callers (that is the whole
/// point: no double-counting of free space); `release` returns exactly what a successful `try_reserve` took.
pub trait StorageReserver: Send + Sync {
    fn try_reserve(&self, purpose: StoragePurpose, bytes: u64) -> Result<(), ReserveRefused>;
    fn release(&self, purpose: StoragePurpose, bytes: u64);
}

/// RAII handle: the bytes stay reserved until this is dropped (or [`StorageReservation::release_now`]).
#[must_use = "dropping a reservation releases it immediately"]
pub struct StorageReservation {
    reserver: Arc<dyn StorageReserver>,
    purpose: StoragePurpose,
    bytes: u64,
}

impl StorageReservation {
    pub fn acquire(reserver: &Arc<dyn StorageReserver>, purpose: StoragePurpose, bytes: u64) -> Result<Self, ReserveRefused> {
        reserver.try_reserve(purpose, bytes)?;
        Ok(Self { reserver: reserver.clone(), purpose, bytes })
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn purpose(&self) -> StoragePurpose {
        self.purpose
    }
    pub fn release_now(self) {}
}

impl Drop for StorageReservation {
    fn drop(&mut self) {
        self.reserver.release(self.purpose, self.bytes);
    }
}

impl fmt::Debug for StorageReservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StorageReservation").field("purpose", &self.purpose).field("bytes", &self.bytes).finish()
    }
}

/// Grants everything — the default when no capacity manager is configured (today's behaviour).
#[derive(Debug, Default, Clone, Copy)]
pub struct UnboundedReserver;

impl StorageReserver for UnboundedReserver {
    fn try_reserve(&self, _purpose: StoragePurpose, _bytes: u64) -> Result<(), ReserveRefused> {
        Ok(())
    }
    fn release(&self, _purpose: StoragePurpose, _bytes: u64) {}
}

/// A fixed byte budget shared by every reservation made through it. Reference implementation for the capacity lane and
/// the scaled-down cap the tests run under.
#[derive(Debug)]
pub struct FixedBudgetReserver {
    budget: u64,
    used: Mutex<u64>,
    high_water: Mutex<u64>,
}

impl FixedBudgetReserver {
    pub fn new(budget: u64) -> Self {
        Self { budget, used: Mutex::new(0), high_water: Mutex::new(0) }
    }
    pub fn used(&self) -> u64 {
        *self.used.lock()
    }
    pub fn high_water(&self) -> u64 {
        *self.high_water.lock()
    }
    pub fn budget(&self) -> u64 {
        self.budget
    }
}

impl StorageReserver for FixedBudgetReserver {
    fn try_reserve(&self, purpose: StoragePurpose, bytes: u64) -> Result<(), ReserveRefused> {
        let mut used = self.used.lock();
        let available = self.budget.saturating_sub(*used);
        if bytes > available {
            return Err(ReserveRefused { purpose, requested: bytes, available });
        }
        *used += bytes;
        let mut hw = self.high_water.lock();
        *hw = (*hw).max(*used);
        Ok(())
    }
    fn release(&self, _purpose: StoragePurpose, bytes: u64) {
        let mut used = self.used.lock();
        *used = used.saturating_sub(bytes);
    }
}
