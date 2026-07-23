//! Per-prefix database size accounting.
//!
//! Every consensus store shares one RocksDB column family and is separated only
//! by a one-byte [`DatabaseStorePrefixes`] key prefix. `du` therefore reports a
//! single number for the whole consensus directory, which is exactly what
//! happened during the 144 GB incident: the growth was attributable to one
//! prefix (206, the per-block EVM state snapshot), and there was no way to say
//! so from outside the process — even after the fact, because the database had
//! to be deleted to recover the machine.
//!
//! This module answers "which store is that?" from SST metadata, cheaply enough
//! to run on a live node:
//!
//! - Per-prefix byte estimates come from `get_approximate_sizes`, which reads
//!   table metadata rather than data blocks.
//! - Emptiness comes from one seek per prefix. That is the check that matters
//!   for a retired store: "is 206 actually at zero rows now?" is a yes/no
//!   question, and a seek answers it in O(1).
//! - Exact row counts require a full scan and are therefore opt-in.
//!
//! The numbers are estimates. They locate a runaway store; they are not an
//! accounting ledger, and they will not sum to the directory size (memtables,
//! WAL, blobs and obsolete-but-not-yet-compacted SSTs all sit outside them).

use crate::prelude::DB;
use crate::registry::DatabaseStorePrefixes;
use num_traits::FromPrimitive;
use rocksdb::Range;

/// One store's footprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefixSize {
    pub prefix: u8,
    /// The [`DatabaseStorePrefixes`] variant name, so output stays in step with
    /// the registry instead of drifting from a hand-maintained table.
    pub name: String,
    /// Approximate live bytes in this prefix's key range.
    pub approx_bytes: u64,
    /// Whether the range holds at least one key. Cheap and exact, unlike
    /// `approx_bytes` — a retired store reads as `false` here even while its
    /// estimate is still non-zero because compaction has not caught up.
    pub non_empty: bool,
    /// Exact row count. `None` unless explicitly requested (it is a full scan).
    pub rows: Option<u64>,
}

/// Whole-database counters, alongside the per-prefix breakdown.
#[derive(Clone, Debug, Default)]
pub struct DbSizeStats {
    pub total_sst_bytes: Option<u64>,
    pub live_data_bytes: Option<u64>,
    /// Bytes RocksDB still owes compaction. A large and RISING value is the
    /// signal that deletes are logical only and the space has not gone back to
    /// the OS yet — the gap between "pruned" and "`du` got smaller".
    pub pending_compaction_bytes: Option<u64>,
    pub estimate_num_keys: Option<u64>,
    pub prefixes: Vec<PrefixSize>,
}

impl DbSizeStats {
    pub fn prefix(&self, prefix: u8) -> Option<&PrefixSize> {
        self.prefixes.iter().find(|p| p.prefix == prefix)
    }
}

/// Every prefix the registry knows about, as `(byte, variant name)`.
///
/// Derived from the enum rather than listed by hand, so a store added later is
/// covered without anyone remembering to update this. The separator (`u8::MAX`)
/// is excluded: it is a key delimiter, not a store, and its range would have no
/// upper bound to scan to.
pub fn registered_prefixes() -> Vec<(u8, String)> {
    (0u8..u8::MAX).filter_map(|byte| DatabaseStorePrefixes::from_u8(byte).map(|variant| (byte, format!("{variant:?}")))).collect()
}

/// Collect size statistics. `count_rows` triggers a full scan of every prefix —
/// minutes on a large database, so it is never the default.
pub fn collect(db: &DB, count_rows: bool) -> DbSizeStats {
    let registered = registered_prefixes();

    // A one-byte prefix owns the key range [p, p+1). `registered_prefixes` excludes
    // 255, so the upper bound never wraps.
    let starts: Vec<[u8; 1]> = registered.iter().map(|(p, _)| [*p]).collect();
    let ends: Vec<[u8; 1]> = registered.iter().map(|(p, _)| [*p + 1]).collect();
    let ranges: Vec<Range<'_>> = starts.iter().zip(ends.iter()).map(|(s, e)| Range::new(s.as_slice(), e.as_slice())).collect();
    let sizes = db.get_approximate_sizes(&ranges);

    let prefixes = registered
        .into_iter()
        .enumerate()
        .map(|(i, (prefix, name))| {
            let approx_bytes = sizes.get(i).copied().unwrap_or(0);
            let (non_empty, rows) = probe_prefix(db, prefix, count_rows);
            PrefixSize { prefix, name, approx_bytes, non_empty, rows }
        })
        .collect();

    DbSizeStats {
        total_sst_bytes: db.property_int_value(rocksdb::properties::TOTAL_SST_FILES_SIZE).ok().flatten(),
        live_data_bytes: db.property_int_value(rocksdb::properties::ESTIMATE_LIVE_DATA_SIZE).ok().flatten(),
        pending_compaction_bytes: db.property_int_value(rocksdb::properties::ESTIMATE_PENDING_COMPACTION_BYTES).ok().flatten(),
        estimate_num_keys: db.property_int_value(rocksdb::properties::ESTIMATE_NUM_KEYS).ok().flatten(),
        prefixes,
    }
}

/// `(non_empty, rows)` for one prefix. Without `count_rows` this reads at most
/// one key.
fn probe_prefix(db: &DB, prefix: u8, count_rows: bool) -> (bool, Option<u64>) {
    let mut iter = db.iterator(rocksdb::IteratorMode::From(&[prefix], rocksdb::Direction::Forward));
    let in_range = |key: &[u8]| key.first() == Some(&prefix);

    if !count_rows {
        let non_empty = matches!(iter.next(), Some(Ok((key, _))) if in_range(&key));
        return (non_empty, None);
    }

    let mut rows = 0u64;
    for item in iter {
        match item {
            Ok((key, _)) if in_range(&key) => rows += 1,
            // The iterator walks the whole column family, so the first key outside
            // this prefix ends the store.
            _ => break,
        }
    }
    (rows > 0, Some(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_temp_db;
    use crate::prelude::ConnBuilder;

    #[test]
    fn registered_prefixes_are_derived_from_the_registry_and_exclude_the_separator() {
        let prefixes = registered_prefixes();
        let byte_of = |name: &str| prefixes.iter().find(|(_, n)| n == name).map(|(b, _)| *b);

        // Spot-check the two that matter for the EVM storage story.
        assert_eq!(byte_of("EvmStateDiff"), Some(206));
        assert_eq!(byte_of("EvmFlatAccount"), Some(234));
        // The separator is a key delimiter, not a store.
        assert!(!prefixes.iter().any(|(b, _)| *b == u8::MAX));
        // Names come from the enum, so they cannot drift from the registry.
        assert!(prefixes.iter().any(|(b, n)| *b == 1 && n == "AcceptanceData"));
    }

    #[test]
    fn collect_attributes_bytes_and_emptiness_to_the_right_prefix() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        // Write into one prefix only.
        const USED: u8 = DatabaseStorePrefixes::EvmStateDiff as u8;
        const UNUSED: u8 = DatabaseStorePrefixes::EvmFlatAccount as u8;
        for i in 0u16..64 {
            let mut key = vec![USED];
            key.extend_from_slice(&i.to_be_bytes());
            db.put(&key, vec![7u8; 4096]).unwrap();
        }
        db.flush().unwrap();

        let stats = collect(&db, true);
        let used = stats.prefix(USED).expect("registered prefix");
        let unused = stats.prefix(UNUSED).expect("registered prefix");

        assert!(used.non_empty);
        assert_eq!(used.rows, Some(64));
        // The emptiness probe is the reliable signal; the byte estimate is not.
        assert!(!unused.non_empty);
        assert_eq!(unused.rows, Some(0));
    }

    #[test]
    fn cheap_collect_reports_emptiness_without_counting() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        const USED: u8 = DatabaseStorePrefixes::EvmStateDiff as u8;
        db.put([USED, 1], vec![0u8; 16]).unwrap();
        db.flush().unwrap();

        let stats = collect(&db, false);
        let used = stats.prefix(USED).unwrap();
        assert!(used.non_empty);
        assert_eq!(used.rows, None, "row counts must stay opt-in — they are a full scan");
    }
}
