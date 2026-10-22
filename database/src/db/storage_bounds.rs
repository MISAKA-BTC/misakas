//! **RocksDB footprint bounds for a node under a storage cap** (G14 capacity lane,
//! `docs/design/palw/storage/capacity.md` §3).
//!
//! RocksDB's defaults are tuned for throughput on a disk with room to spare: obsolete files are
//! purged on a 6-hour period, a single compaction may write up to 25 × the target file size before
//! its inputs are deleted, the MANIFEST may grow to 1 GiB, and up to 1000 info logs (`LOG.old.*`)
//! are kept per database — one per restart, forever. Under a byte cap each of those is unaccounted
//! peak space. These bounds are applied ONLY when the node runs with `--storage-cap-bytes`
//! (`None` otherwise, so an uncapped node opens every DB exactly as before). They change no on-disk
//! format: SST, WAL and MANIFEST files written under them are read by an unbounded node unchanged.
use rocksdb::Options;

/// The knobs, per database class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageDbBounds {
    /// Flush memtables once the live WALs exceed this (bounds the WAL tail of the DB).
    pub max_total_wal_size: u64,
    /// SST target size: the unit a compaction rewrites.
    pub target_file_size_base: u64,
    /// Upper bound on the bytes one compaction may produce before its inputs are released — the
    /// compaction's transient space amplification.
    pub max_compaction_bytes: u64,
    /// How often files no longer referenced are purged (RocksDB default: 6 h).
    pub delete_obsolete_files_period_micros: u64,
    pub compaction_readahead_size: usize,
    /// The MANIFEST is rolled over at this size (RocksDB default: 1 GiB).
    pub max_manifest_file_size: usize,
    /// Info logs kept (`LOG`, `LOG.old.*`; RocksDB default: 1000).
    pub keep_log_file_num: usize,
    /// One info log rolls at this size (RocksDB default: unbounded).
    pub max_log_file_size: usize,
    /// Incremental fsync of SST/WAL writes, so a low-space stop flushes little.
    pub bytes_per_sync: u64,
    /// Background jobs (flushes + compactions): bounds how many compactions hold transient output
    /// at once. Overrides the parallelism the preset derived from the core count.
    pub max_background_jobs: i32,
}

impl StorageDbBounds {
    /// The active and staging consensus databases.
    pub const CONSENSUS: StorageDbBounds = StorageDbBounds {
        max_total_wal_size: 512 << 20,
        target_file_size_base: 64 << 20,
        max_compaction_bytes: 512 << 20,
        delete_obsolete_files_period_micros: 5 * 60 * 1_000_000,
        compaction_readahead_size: 2 << 20,
        max_manifest_file_size: 64 << 20,
        keep_log_file_num: 3,
        max_log_file_size: 32 << 20,
        bytes_per_sync: 1 << 20,
        max_background_jobs: 4,
    };

    /// The small databases (meta, utxo index, address manager).
    pub const SMALL: StorageDbBounds = StorageDbBounds {
        max_total_wal_size: 64 << 20,
        target_file_size_base: 32 << 20,
        max_compaction_bytes: 128 << 20,
        delete_obsolete_files_period_micros: 5 * 60 * 1_000_000,
        compaction_readahead_size: 2 << 20,
        max_manifest_file_size: 16 << 20,
        keep_log_file_num: 2,
        max_log_file_size: 8 << 20,
        bytes_per_sync: 1 << 20,
        max_background_jobs: 2,
    };

    /// Worst-case transient bytes one DB can hold beyond its live data under these bounds: the WAL
    /// tail, `max_background_jobs` concurrent compactions' outputs (one job may be a flush; counting
    /// all of them is the conservative figure), the MANIFEST and the info logs. The figure the
    /// WAL/compaction budget is planned against (not a measurement).
    pub fn transient_bound(&self) -> u64 {
        let concurrent_compactions = self.max_background_jobs.max(1) as u64;
        self.max_total_wal_size
            .saturating_add(self.max_compaction_bytes.saturating_mul(concurrent_compactions.max(1)))
            .saturating_add(self.max_manifest_file_size as u64)
            .saturating_add((self.keep_log_file_num as u64).saturating_mul(self.max_log_file_size as u64))
    }

    pub fn apply_to_options(&self, opts: &mut Options) {
        opts.set_max_total_wal_size(self.max_total_wal_size);
        opts.set_target_file_size_base(self.target_file_size_base);
        opts.set_max_compaction_bytes(self.max_compaction_bytes);
        opts.set_delete_obsolete_files_period_micros(self.delete_obsolete_files_period_micros);
        opts.set_compaction_readahead_size(self.compaction_readahead_size);
        opts.set_max_manifest_file_size(self.max_manifest_file_size);
        opts.set_keep_log_file_num(self.keep_log_file_num);
        opts.set_max_log_file_size(self.max_log_file_size);
        opts.set_bytes_per_sync(self.bytes_per_sync);
        opts.set_wal_bytes_per_sync(self.bytes_per_sync);
        opts.set_max_background_jobs(self.max_background_jobs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::ConnBuilder;

    fn info_logs(dir: &std::path::Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n == "LOG" || n.starts_with("LOG.old")
            })
            .count()
    }

    /// Every restart of an unbounded DB leaves one more `LOG.old.*` (up to RocksDB's 1000); a
    /// bounded DB keeps `keep_log_file_num`. Data written under the bounds reads back unchanged.
    #[test]
    fn bounded_dbs_keep_a_fixed_number_of_info_logs_across_restarts_and_read_back_their_data() {
        let dir = tempfile::tempdir().unwrap();
        let bounded = dir.path().join("bounded");
        let unbounded = dir.path().join("unbounded");
        for i in 0..6u8 {
            for (path, bounds) in [(&bounded, Some(StorageDbBounds::SMALL)), (&unbounded, None)] {
                let db = ConnBuilder::default()
                    .with_db_path(path.clone())
                    .with_files_limit(10)
                    .with_storage_bounds(bounds)
                    .build()
                    .unwrap();
                db.put([i], [i; 64]).unwrap();
                db.flush().unwrap();
                drop(db);
                // RocksDB names `LOG.old.<micros>`; a restart in the same microsecond would collide.
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        assert!(info_logs(&bounded) <= StorageDbBounds::SMALL.keep_log_file_num, "bounded: {}", info_logs(&bounded));
        assert!(info_logs(&unbounded) >= 6, "unbounded keeps one per restart: {}", info_logs(&unbounded));
        // The unbounded node reads the bounded node's files (no format change).
        let db = ConnBuilder::default().with_db_path(bounded.clone()).with_files_limit(10).build().unwrap();
        for i in 0..6u8 {
            assert_eq!(db.get([i]).unwrap().as_deref(), Some(&[i; 64][..]));
        }
    }

    #[test]
    fn the_planned_transient_bound_fits_the_30gb_wal_and_maintenance_budget() {
        // The active consensus DB plus the three small DBs share the 4 GB WAL/compaction budget; a
        // staging DB's transient bytes are inside its own (envelope) reservation.
        let active = StorageDbBounds::CONSENSUS.transient_bound() + 3 * StorageDbBounds::SMALL.transient_bound();
        assert!(active <= 4_000_000_000, "planned transient bound {active}");
        assert!(StorageDbBounds::CONSENSUS.transient_bound() <= 10_497_346_816 / 3);
    }
}
