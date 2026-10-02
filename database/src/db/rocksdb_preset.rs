//! RocksDB configuration presets for different use cases
//!
//! This module provides pre-configured RocksDB option sets optimized for different
//! deployment scenarios.

use rocksdb::Options;
use std::str::FromStr;

/// The block cache a CONSENSUS database gets under the default preset when `--rocksdb-cache-size` does not name one:
/// 256 MiB in [`BLOCK_CACHE_SHARD_BITS`] = 1 bit (two 128 MiB shards) — more than twice the ~57 MB PALW tip row's block, so
/// the block and the rows beside it stay resident while the data the node actually reads fills the rest. The small
/// databases (address manager, meta, utxo index) keep RocksDB's own default.
pub const DEFAULT_PRESET_CONSENSUS_BLOCK_CACHE_BYTES: usize = 256 * 1024 * 1024;

/// **Why two shards.** RocksDB splits an LRU cache into `2^bits` shards and an entry bigger than ONE shard is never
/// kept: its default (`bits` chosen for 512 KB shards, so 6 for any cache over 32 MB) makes the built-in 32 MB cache
/// 64 shards of 512 KB, and a 192 MB one 3 MB shards — a 57 MB block fits in neither. Two shards keep the lock
/// contention of a hot read path low and give each shard room for the block.
pub const BLOCK_CACHE_SHARD_BITS: i32 = 1;

/// Available RocksDB configuration presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RocksDbPreset {
    /// Default configuration - balanced for general use on SSD/NVMe
    /// - 64MB write buffer
    /// - Standard compression
    /// - Optimized for fast storage
    #[default]
    Default,

    /// HDD configuration - optimized for hard disk drives
    /// - 256MB write buffer (4x default)
    /// - Aggressive compression (LZ4 + ZSTD)
    /// - BlobDB enabled for large values
    /// - Rate limiting to prevent I/O spikes
    /// - Optimized for sequential writes and reduced write amplification
    ///
    /// Recommended for archival nodes on HDD storage.
    Hdd,
}

impl FromStr for RocksDbPreset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(Self::Default),
            "hdd" => Ok(Self::Hdd),
            _ => Err(format!("Unknown RocksDB preset: '{}'. Valid options: default, hdd", s)),
        }
    }
}

impl std::fmt::Display for RocksDbPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::Hdd => write!(f, "hdd"),
        }
    }
}

impl RocksDbPreset {
    /// Apply the preset configuration to RocksDB options
    ///
    /// # Arguments
    /// * `opts` - RocksDB options to configure
    /// * `parallelism` - Number of background threads
    /// * `mem_budget` - Memory budget (only used for Default preset, HDD uses fixed 256MB)
    pub fn apply_to_options(&self, opts: &mut Options, parallelism: usize, mem_budget: usize, cache_budget: Option<usize>) {
        match self {
            Self::Default => self.apply_default(opts, parallelism, mem_budget, cache_budget),
            Self::Hdd => self.apply_hdd(opts, parallelism, cache_budget),
        }
    }

    /// Apply default preset configuration
    fn apply_default(&self, opts: &mut Options, parallelism: usize, mem_budget: usize, cache_budget: Option<usize>) {
        if parallelism > 1 {
            opts.increase_parallelism(parallelism as i32);
        }

        // Use the provided memory budget (typically 64MB)
        opts.optimize_level_style_compaction(mem_budget);

        // **A block cache that can hold the largest block this database writes** — when the caller names one
        // (`cache_budget`: `--rocksdb-cache-size`, or the consensus factory's default,
        // [`DEFAULT_PRESET_CONSENSUS_BLOCK_CACHE_BYTES`]) (the 2026-10-03 panel
        // starvation, docs/design/palw/t12-panel-backlog-1003.md §4). RocksDB's own default is a 32 MB LRU cache of
        // 64 shards of 512 KB, and a cache never keeps an entry bigger than ONE shard: the PALW chain state's tip row is rewritten
        // at every virtual commit and is ~57 MB, so the data block that carries it (and the small rows that sort
        // beside it) was read from disk, 57 MB at a time, on EVERY point read that fell in it — measured at ten
        // 60 MB `pread`s a second on the virtual-processor thread of the 5.104 seats. Every other table option
        // stays RocksDB's default, so existing files, filters and indexes read exactly as before; only the cache
        // changes, and it is filled lazily (the resident size is what the node actually reads).
        if let Some(cache_bytes) = cache_budget {
            use rocksdb::{BlockBasedOptions, Cache, LruCacheOptions};
            let mut cache_opts = LruCacheOptions::default();
            cache_opts.set_capacity(cache_bytes);
            cache_opts.set_num_shard_bits(BLOCK_CACHE_SHARD_BITS);
            let mut block_opts = BlockBasedOptions::default();
            block_opts.set_block_cache(&Cache::new_lru_cache_opts(&cache_opts));
            opts.set_block_based_table_factory(&block_opts);
        }
    }

    /// Apply HDD preset configuration (HDD-optimized settings)
    fn apply_hdd(&self, opts: &mut Options, parallelism: usize, cache_budget: Option<usize>) {
        if parallelism > 1 {
            opts.increase_parallelism(parallelism as i32);
        }

        // Memory and write buffer settings (256MB for better batching on HDD)
        let write_buffer_size = 256 * 1024 * 1024; // 256MB

        // Optimize for level-style compaction with archive-appropriate memory
        // This sets up LSM tree parameters
        opts.optimize_level_style_compaction(write_buffer_size);

        // Re-set write_buffer_size after optimize_level_style_compaction()
        // because optimize_level_style_compaction() internally overrides it to size/4
        opts.set_write_buffer_size(write_buffer_size);

        // LSM Tree Structure - Optimized for large (4TB+) archives
        // 256 MB SST files reduce file count dramatically (500K → 16K files for 4TB)
        opts.set_target_file_size_base(256 * 1024 * 1024); // 256 MB SST files
        opts.set_target_file_size_multiplier(1); // Same size across all levels
        opts.set_max_bytes_for_level_base(1024 * 1024 * 1024); // 1 GB L1 base
        opts.set_level_compaction_dynamic_level_bytes(true); // Minimize space amplification

        // Compaction settings
        // Trigger compaction when L0 has just 1 file (minimize write amplification)
        opts.set_level_zero_file_num_compaction_trigger(1);

        // Prioritize compacting older/smaller files first
        use rocksdb::CompactionPri;
        opts.set_compaction_pri(CompactionPri::OldestSmallestSeqFirst);

        // Read-ahead for compactions (4MB - good for sequential HDD reads)
        opts.set_compaction_readahead_size(4 * 1024 * 1024);

        // Compression strategy: LZ4 for all levels, ZSTD for bottommost
        use rocksdb::DBCompressionType;

        // Set default compression to LZ4 (fast)
        opts.set_compression_type(DBCompressionType::Lz4);

        // Enable bottommost level compression with maximum ZSTD level
        opts.set_bottommost_compression_type(DBCompressionType::Zstd);

        // ZSTD compression options for bottommost level
        // Larger dictionaries (64 KB) improve compression on large archives
        opts.set_compression_options(
            -1,        // window_bits (let ZSTD choose optimal)
            22,        // level (maximum compression)
            0,         // strategy (default)
            64 * 1024, // dict_bytes (64 KB dictionary)
        );

        // Train ZSTD dictionaries on 8 MB of sample data (~125x dictionary size)
        opts.set_zstd_max_train_bytes(8 * 1024 * 1024);

        // Block-based table options for better caching
        use rocksdb::{BlockBasedOptions, Cache};
        let mut block_opts = BlockBasedOptions::default();

        // Partitioned Bloom filters (18 bits per key for better false-positive rate)
        block_opts.set_bloom_filter(18.0, false); // 18 bits per key
        block_opts.set_partition_filters(true); // Partition for large databases
        block_opts.set_format_version(5); // Latest format with optimizations
        block_opts.set_index_type(rocksdb::BlockBasedIndexType::TwoLevelIndexSearch);

        // Cache index and filter blocks in block cache for faster queries
        block_opts.set_cache_index_and_filter_blocks(true);

        // Block cache: Default 256MB (safe for low-RAM systems)
        // Can be scaled via ram-scale or overridden via --rocksdb-cache-size
        let default_cache_size = 256 * 1024 * 1024; // 256MB
        let cache_size = cache_budget.unwrap_or(default_cache_size);
        let cache = Cache::new_lru_cache(cache_size);
        block_opts.set_block_cache(&cache);

        // Set block size (256KB - better for sequential HDD reads)
        block_opts.set_block_size(256 * 1024);

        opts.set_block_based_table_factory(&block_opts);

        // Rate limiting: prevent I/O spikes on HDD
        // 12 MB/s rate limit for background writes
        opts.set_ratelimiter(12 * 1024 * 1024, 100_000, 10);

        // Enable BlobDB for large values (reduces write amplification)
        opts.set_enable_blob_files(true);
        opts.set_min_blob_size(512); // Only values >512 bytes go to blob files
        opts.set_blob_file_size(256 * 1024 * 1024); // 256MB blob files
        opts.set_blob_compression_type(DBCompressionType::Zstd); // Compress blobs
        opts.set_enable_blob_gc(true); // Enable garbage collection
        opts.set_blob_gc_age_cutoff(0.9); // GC blobs when 90% old
        opts.set_blob_gc_force_threshold(0.1); // Force GC at 10% garbage
        opts.set_blob_compaction_readahead_size(8 * 1024 * 1024); // 8 MB blob readahead
    }

    /// Get a human-readable description of the preset
    pub fn description(&self) -> &'static str {
        match self {
            Self::Default => "Default preset - balanced for SSD/NVMe (64MB write buffer, standard compression)",
            Self::Hdd => {
                "HDD preset - optimized for hard disk drives (256MB write buffer, BlobDB, aggressive compression, rate limiting)"
            }
        }
    }

    /// Get the recommended use case for this preset
    pub fn use_case(&self) -> &'static str {
        match self {
            Self::Default => "General purpose nodes on SSD/NVMe storage",
            Self::Hdd => "Archival nodes on HDD storage (--archival flag recommended)",
        }
    }

    /// Get memory requirements for this preset
    pub fn memory_requirements(&self) -> &'static str {
        match self {
            Self::Default => "~4GB minimum, scales with --ram-scale",
            Self::Hdd => "~4GB minimum (256MB write buffer + 256MB cache + overhead), 8GB+ recommended for public RPC",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resident bytes of RocksDB's block cache after reading `big_key`'s 40 MB value back, under `cache_budget`.
    fn block_cache_usage_after_reading_a_40_mb_row(cache_budget: Option<usize>) -> u64 {
        use crate::prelude::{ConnBuilder, RocksDbPreset};
        use rand::RngCore;
        let (_life, db) = crate::create_temp_db!(ConnBuilder::default().with_files_limit(10).with_preset(RocksDbPreset::Default).with_cache_budget(cache_budget));
        let mut big = vec![0u8; 40 << 20];
        rand::thread_rng().fill_bytes(&mut big);
        db.put(b"a-small", b"1").unwrap();
        db.put(b"b-big", &big).unwrap();
        db.put(b"c-small", b"2").unwrap();
        db.flush().unwrap();
        // Several reads of a row that shares the big row's data block, and of the big row itself.
        for _ in 0..3 {
            assert_eq!(db.get(b"a-small").unwrap().as_deref(), Some(&b"1"[..]));
            assert_eq!(db.get(b"b-big").unwrap().map(|v| v.len()), Some(40 << 20));
            assert_eq!(db.get(b"c-small").unwrap().as_deref(), Some(&b"2"[..]));
        }
        db.property_int_value("rocksdb.block-cache-usage"),
            db.property_int_value("rocksdb.block-cache-pinned-usage")
        );
        db.property_int_value("rocksdb.block-cache-usage").unwrap().unwrap_or(0)
    }

    /// The 2026-10-03 finding: RocksDB's built-in 32 MB block cache cannot keep a data block bigger than itself, so every
    /// point read of a row beside the ~57 MB PALW tip row re-read it from disk. A cache sized for the consensus database
    /// keeps the block, and the rows beside it, resident.
    #[test]
    fn a_consensus_sized_block_cache_keeps_a_block_bigger_than_rocksdbs_default_and_the_default_does_not() {
        let default_cache = block_cache_usage_after_reading_a_40_mb_row(None);
        let sized = block_cache_usage_after_reading_a_40_mb_row(Some(DEFAULT_PRESET_CONSENSUS_BLOCK_CACHE_BYTES));
        assert!(default_cache < 32 << 20, "the built-in 32 MB cache cannot hold a 40 MB block ({default_cache} bytes resident)");
        assert!(sized >= 40 << 20, "the sized cache holds the whole block ({sized} bytes resident)");
    }

    #[test]
    fn test_preset_from_str() {
        assert_eq!(RocksDbPreset::from_str("default").unwrap(), RocksDbPreset::Default);
        assert_eq!(RocksDbPreset::from_str("Default").unwrap(), RocksDbPreset::Default);
        assert_eq!(RocksDbPreset::from_str("hdd").unwrap(), RocksDbPreset::Hdd);
        assert_eq!(RocksDbPreset::from_str("HDD").unwrap(), RocksDbPreset::Hdd);
        assert!(RocksDbPreset::from_str("unknown").is_err());
    }
}
