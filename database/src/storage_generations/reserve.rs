//! **Storage reservations** — the narrow interface the generation store and the spill area reserve through.
//!
//! G14 storage (2026-10-22): before a large receive, unpack or DB switch the node estimates the bytes it is about to
//! write and RESERVES them, so two concurrent tasks cannot both count the same free space. The capacity lane owns the
//! node's real manager (`kaspa_utils::storage_budget::StorageBudget`, RAII `StorageReservation`, categories
//! `StagingDb` / `IbdSpill`, on that lane's branch at the time this was written). This file deliberately stays a
//! one-method trait so the adapter at integration is a few lines:
//!
//! ```ignore
//! impl StorageReserver for StorageBudgetReserver /* wraps Arc<kaspa_utils::storage_budget::StorageBudget> */ {
//!     fn reserve(&self, purpose: StoragePurpose, bytes: u64) -> Result<ReservationGuard, ReserveRefused> {
//!         let category = match purpose { StoragePurpose::StagingGeneration => StorageCategory::StagingDb,
//!                                        StoragePurpose::IbdSpill => StorageCategory::IbdSpill,
//!                                        StoragePurpose::Other(_) => StorageCategory::Other };
//!         self.0.reserve_after_reclaim(category, bytes).map(|r| Box::new(r) as ReservationGuard)
//!             .map_err(|why| ReserveRefused { purpose, requested: bytes, available: 0, reason: why.to_string() })
//!     }
//! }
//! ```
//!
//! Default-safe: [`UnboundedReserver`] grants everything (today's behaviour). [`FixedBudgetReserver`] is the
//! reference implementation the tests run their scaled-down caps under.

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

/// A refused reservation. Capacity shortage is a LOCAL condition: callers turn it into "pause / retry", never into a
/// verdict on a peer or a block.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("storage reservation refused for {purpose}: requested {requested} bytes, {available} available {reason}")]
pub struct ReserveRefused {
    pub purpose: StoragePurpose,
    pub requested: u64,
    pub available: u64,
    pub reason: String,
}

/// Whatever the manager hands back; the bytes stay reserved until it is dropped.
pub type ReservationGuard = Box<dyn Send>;

/// The capacity manager's side. Must be atomic with respect to concurrent callers (no double-counting of free space).
pub trait StorageReserver: Send + Sync {
    fn reserve(&self, purpose: StoragePurpose, bytes: u64) -> Result<ReservationGuard, ReserveRefused>;
}

/// RAII handle: the bytes stay reserved until this is dropped.
#[must_use = "dropping a reservation releases it immediately"]
pub struct StorageReservation {
    _guard: ReservationGuard,
    purpose: StoragePurpose,
    bytes: u64,
}

impl StorageReservation {
    pub fn acquire(reserver: &Arc<dyn StorageReserver>, purpose: StoragePurpose, bytes: u64) -> Result<Self, ReserveRefused> {
        Ok(Self { _guard: reserver.reserve(purpose, bytes)?, purpose, bytes })
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn purpose(&self) -> StoragePurpose {
        self.purpose
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
    fn reserve(&self, _purpose: StoragePurpose, _bytes: u64) -> Result<ReservationGuard, ReserveRefused> {
        Ok(Box::new(()))
    }
}

#[derive(Debug, Default)]
struct Ledger {
    used: u64,
    high_water: u64,
}

/// A fixed byte budget shared by every reservation made through it.
#[derive(Debug)]
pub struct FixedBudgetReserver {
    budget: u64,
    ledger: Arc<Mutex<Ledger>>,
}

struct FixedGuard {
    ledger: Arc<Mutex<Ledger>>,
    bytes: u64,
}

impl Drop for FixedGuard {
    fn drop(&mut self) {
        let mut l = self.ledger.lock();
        l.used = l.used.saturating_sub(self.bytes);
    }
}

impl FixedBudgetReserver {
    pub fn new(budget: u64) -> Self {
        Self { budget, ledger: Default::default() }
    }
    pub fn used(&self) -> u64 {
        self.ledger.lock().used
    }
    pub fn high_water(&self) -> u64 {
        self.ledger.lock().high_water
    }
    pub fn budget(&self) -> u64 {
        self.budget
    }
}

impl StorageReserver for FixedBudgetReserver {
    fn reserve(&self, purpose: StoragePurpose, bytes: u64) -> Result<ReservationGuard, ReserveRefused> {
        let mut l = self.ledger.lock();
        let available = self.budget.saturating_sub(l.used);
        if bytes > available {
            return Err(ReserveRefused { purpose, requested: bytes, available, reason: String::new() });
        }
        l.used += bytes;
        l.high_water = l.high_water.max(l.used);
        Ok(Box::new(FixedGuard { ledger: self.ledger.clone(), bytes }))
    }
}
