use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct ProcessingCounters {
    pub blocks_submitted: AtomicU64,
    pub header_counts: AtomicU64,
    pub dep_counts: AtomicU64,
    pub mergeset_counts: AtomicU64,
    pub body_counts: AtomicU64,
    pub txs_counts: AtomicU64,
    pub chain_block_counts: AtomicU64,
    pub chain_disqualified_counts: AtomicU64,
    pub mass_counts: AtomicU64,
    // IBD header-processing timing instrumentation (consensus-neutral; measures the serial
    // reachability-commit fraction `f` per docs/design-parallel-ibd-ghostdag.md §7-1).
    /// ns spent in `validate_header` (Phase-A: parallelizable compute incl. GHOSTDAG).
    pub hdr_validate_ns: AtomicU64,
    /// ns spent in `commit_header` (the serial committer critical section).
    pub hdr_commit_ns: AtomicU64,
    /// ns spent in `reachability::add_block` (core serial reachability mutation, incl. reindex bursts).
    pub hdr_addblock_ns: AtomicU64,
    /// ns spent in the `db.write(batch)` fsync of the header commit.
    pub hdr_dbwrite_ns: AtomicU64,
    /// ns the reachability `upgradable_read` lock is held (acquire → after db.write).
    pub hdr_heldlock_ns: AtomicU64,
    /// count of ordinary headers timed (denominator for the per-header averages).
    pub hdr_timed_counts: AtomicU64,
}

impl ProcessingCounters {
    pub fn snapshot(&self) -> ProcessingCountersSnapshot {
        ProcessingCountersSnapshot {
            blocks_submitted: self.blocks_submitted.load(Ordering::Relaxed),
            header_counts: self.header_counts.load(Ordering::Relaxed),
            dep_counts: self.dep_counts.load(Ordering::Relaxed),
            mergeset_counts: self.mergeset_counts.load(Ordering::Relaxed),
            body_counts: self.body_counts.load(Ordering::Relaxed),
            txs_counts: self.txs_counts.load(Ordering::Relaxed),
            chain_block_counts: self.chain_block_counts.load(Ordering::Relaxed),
            chain_disqualified_counts: self.chain_disqualified_counts.load(Ordering::Relaxed),
            mass_counts: self.mass_counts.load(Ordering::Relaxed),
            hdr_validate_ns: self.hdr_validate_ns.load(Ordering::Relaxed),
            hdr_commit_ns: self.hdr_commit_ns.load(Ordering::Relaxed),
            hdr_addblock_ns: self.hdr_addblock_ns.load(Ordering::Relaxed),
            hdr_dbwrite_ns: self.hdr_dbwrite_ns.load(Ordering::Relaxed),
            hdr_heldlock_ns: self.hdr_heldlock_ns.load(Ordering::Relaxed),
            hdr_timed_counts: self.hdr_timed_counts.load(Ordering::Relaxed),
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct ProcessingCountersSnapshot {
    pub blocks_submitted: u64,
    pub header_counts: u64,
    pub dep_counts: u64,
    pub mergeset_counts: u64,
    pub body_counts: u64,
    pub txs_counts: u64,
    pub chain_block_counts: u64,
    pub chain_disqualified_counts: u64,
    pub mass_counts: u64,
    pub hdr_validate_ns: u64,
    pub hdr_commit_ns: u64,
    pub hdr_addblock_ns: u64,
    pub hdr_dbwrite_ns: u64,
    pub hdr_heldlock_ns: u64,
    pub hdr_timed_counts: u64,
}

impl core::ops::Sub for &ProcessingCountersSnapshot {
    type Output = ProcessingCountersSnapshot;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            blocks_submitted: self.blocks_submitted.saturating_sub(rhs.blocks_submitted),
            header_counts: self.header_counts.saturating_sub(rhs.header_counts),
            dep_counts: self.dep_counts.saturating_sub(rhs.dep_counts),
            mergeset_counts: self.mergeset_counts.saturating_sub(rhs.mergeset_counts),
            body_counts: self.body_counts.saturating_sub(rhs.body_counts),
            txs_counts: self.txs_counts.saturating_sub(rhs.txs_counts),
            chain_block_counts: self.chain_block_counts.saturating_sub(rhs.chain_block_counts),
            chain_disqualified_counts: self.chain_disqualified_counts.saturating_sub(rhs.chain_disqualified_counts),
            mass_counts: self.mass_counts.saturating_sub(rhs.mass_counts),
            hdr_validate_ns: self.hdr_validate_ns.saturating_sub(rhs.hdr_validate_ns),
            hdr_commit_ns: self.hdr_commit_ns.saturating_sub(rhs.hdr_commit_ns),
            hdr_addblock_ns: self.hdr_addblock_ns.saturating_sub(rhs.hdr_addblock_ns),
            hdr_dbwrite_ns: self.hdr_dbwrite_ns.saturating_sub(rhs.hdr_dbwrite_ns),
            hdr_heldlock_ns: self.hdr_heldlock_ns.saturating_sub(rhs.hdr_heldlock_ns),
            hdr_timed_counts: self.hdr_timed_counts.saturating_sub(rhs.hdr_timed_counts),
        }
    }
}
