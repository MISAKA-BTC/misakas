//! `kaspad --db-stats` — attribute consensus DB size to individual stores.
//!
//! When the EVM testnet node reached 144 GB, `du` could say "consensus = 144 GB"
//! and nothing could say which of the ~60 stores inside it that was. The answer
//! had to be reconstructed from source, and the database was deleted to recover
//! the machine, so it can never be confirmed. This is the tool that would have
//! answered it in a second.
//!
//! It opens the consensus database READ-ONLY, so it runs against a live node —
//! which is when the question is actually asked — and prints stores by size.
//!
//! It only reads. Nothing here deletes, compacts, or migrates: a diagnostic that
//! can change the thing it is diagnosing is not one an operator can reach for
//! while a node is in trouble.

use crate::args::Args;
use crate::daemon::{CONSENSUS_DB, DEFAULT_DATA_DIR, get_app_dir_from_args};
use kaspa_database::prelude::ConnBuilder;
use kaspa_database::size_stats::{self, DbSizeStats};
use std::path::Path;

/// How many file descriptors the read-only open may take. Small on purpose: this
/// process is a guest next to a running node that already holds an FD budget.
const DB_STATS_FILES_LIMIT: i32 = 128;

pub const MODE_ESTIMATE: &str = "estimate";
pub const MODE_COUNT_ROWS: &str = "count-rows";

/// Scale the unit to the value. A fixed GiB column renders a healthy database as a
/// column of `0.00` and the report becomes unreadable in exactly the case where the
/// operator is checking that a store is small.
fn human_bytes(bytes: u64) -> String {
    const UNITS: [(&str, f64); 4] = [("GiB", 1024.0 * 1024.0 * 1024.0), ("MiB", 1024.0 * 1024.0), ("KiB", 1024.0), ("B", 1.0)];
    for (unit, scale) in UNITS {
        if bytes as f64 >= scale {
            return format!("{:.2} {unit}", bytes as f64 / scale);
        }
    }
    "0 B".to_string()
}

fn opt_bytes(bytes: Option<u64>) -> String {
    bytes.map(human_bytes).unwrap_or_else(|| "unavailable".to_string())
}

/// Render the report. Split from the I/O so the layout is testable.
pub fn render(db_path: &Path, stats: &DbSizeStats, counted_rows: bool) -> String {
    let mut out = String::new();
    out.push_str("MISAKA consensus DB size report\n\n");
    out.push_str(&format!("  path                 {}\n", db_path.display()));
    out.push_str(&format!("  total SST files      {}\n", opt_bytes(stats.total_sst_bytes)));
    out.push_str(&format!("  estimated live data  {}\n", opt_bytes(stats.live_data_bytes)));
    // The gap between "pruned" and "`du` got smaller": deletes are logical until
    // compaction runs, so a large and rising value here explains a directory that
    // will not shrink.
    out.push_str(&format!("  pending compaction   {}\n", opt_bytes(stats.pending_compaction_bytes)));
    out.push_str(&format!(
        "  estimated keys       {}\n\n",
        stats.estimate_num_keys.map(|k| k.to_string()).unwrap_or_else(|| "unavailable".to_string())
    ));

    let mut rows: Vec<_> = stats.prefixes.iter().filter(|p| p.non_empty || p.approx_bytes > 0).collect();
    rows.sort_by(|a, b| b.approx_bytes.cmp(&a.approx_bytes).then(a.prefix.cmp(&b.prefix)));

    out.push_str(&format!("  {:>6}  {:<28}  {:>13}  {:>14}\n", "prefix", "store", "approx size", "rows"));
    out.push_str(&format!("  {:->6}  {:-<28}  {:->13}  {:->14}\n", "", "", "", ""));
    for p in &rows {
        let rows_cell = match p.rows {
            Some(n) => n.to_string(),
            // Without a full scan the honest answer is not "0" but "unknown, and
            // here is whether it is empty" — which is the question that matters.
            None if p.non_empty => "non-empty".to_string(),
            None => "empty".to_string(),
        };
        out.push_str(&format!("  {:>6}  {:<28}  {:>13}  {:>14}\n", p.prefix, p.name, human_bytes(p.approx_bytes), rows_cell));
    }
    if rows.is_empty() {
        out.push_str("  (no store holds any data)\n");
    }

    out.push_str(&format!("\n  {} of {} registered stores hold data.\n", rows.len(), stats.prefixes.len()));
    if !counted_rows {
        out.push_str(&format!("  Row counts are a full scan; re-run with --db-stats={MODE_COUNT_ROWS} for exact numbers.\n"));
    }
    // The one store whose size is a configuration choice rather than a consequence
    // of chain history, and the reason this tool exists.
    if let Some(evm_206) = stats.prefix(kaspa_database::registry::DatabaseStorePrefixes::EvmStateDiff as u8)
        && (evm_206.non_empty || evm_206.rows.is_some_and(|r| r > 0))
    {
        out.push_str(
            "\n  NOTE: prefix 206 (EvmStateDiff) holds rows. Despite its name it stores a FULL EVM state\n\
             \x20 snapshot per block, so it grows as O(state x kept blocks). Start the node with\n\
             \x20 --evm-storage-profile=compact to stop writing it, and --evm-prune-legacy-206 once to\n\
             \x20 reclaim what is already there.\n",
        );
    }
    out
}

/// Every RocksDB under the consensus directory.
///
/// `ConsensusFactory` gives each consensus its own `consensus-NNN` subdirectory, so
/// the path from the args is a container, not a database. All of them are reported:
/// a superseded or staging consensus left on disk is itself a disk-space finding,
/// and hiding it would defeat the purpose of the report. A `CURRENT` file is
/// RocksDB's own marker for "this directory is a database".
fn discover_databases(container: &Path) -> Vec<std::path::PathBuf> {
    if container.join("CURRENT").is_file() {
        return vec![container.to_path_buf()];
    }
    let Ok(entries) = std::fs::read_dir(container) else { return Vec::new() };
    let mut found: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.join("CURRENT").is_file()).collect();
    found.sort();
    found
}

/// Print the report and return the process exit code.
pub fn run(args: &Args, mode: &str) -> i32 {
    let count_rows = mode == MODE_COUNT_ROWS;
    let network = args.network();
    let container = get_app_dir_from_args(args).join(network.to_prefixed()).join(DEFAULT_DATA_DIR).join(CONSENSUS_DB);

    if !container.exists() {
        println!("No consensus database at {} (nothing to report).", container.display());
        return 1;
    }
    let databases = discover_databases(&container);
    if databases.is_empty() {
        println!("No RocksDB database found under {} (nothing to report).", container.display());
        return 1;
    }

    if count_rows {
        println!("Counting rows in every store — this is a full scan and can take minutes on a large database.\n");
    }

    let mut failures = 0;
    for db_path in &databases {
        let db = match ConnBuilder::default().with_db_path(db_path.clone()).with_files_limit(DB_STATS_FILES_LIMIT).build_read_only() {
            Ok(db) => db,
            Err(err) => {
                println!("Could not open {} read-only: {err}\n", db_path.display());
                failures += 1;
                continue;
            }
        };
        print!("{}", render(db_path, &size_stats::collect(&db, count_rows), count_rows));
        println!();
    }
    if failures == databases.len() { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::size_stats::PrefixSize;

    fn prefix(prefix: u8, name: &str, approx_bytes: u64, non_empty: bool, rows: Option<u64>) -> PrefixSize {
        PrefixSize { prefix, name: name.to_string(), approx_bytes, non_empty, rows }
    }

    #[test]
    fn report_sorts_by_size_and_names_the_prefix_that_caused_the_incident() {
        let stats = DbSizeStats {
            total_sst_bytes: Some(150 * 1024 * 1024 * 1024),
            live_data_bytes: Some(140 * 1024 * 1024 * 1024),
            pending_compaction_bytes: Some(2 * 1024 * 1024 * 1024),
            estimate_num_keys: Some(1234),
            prefixes: vec![
                prefix(25, "UtxoDiffs", 6 * 1024 * 1024 * 1024, true, None),
                prefix(206, "EvmStateDiff", 128 * 1024 * 1024 * 1024, true, None),
                prefix(234, "EvmFlatAccount", 0, false, None),
            ],
        };
        let out = render(Path::new("/var/lib/misaka/consensus"), &stats, false);

        // Biggest store first — the point of the report.
        let evm_at = out.find("EvmStateDiff").unwrap();
        let utxo_at = out.find("UtxoDiffs").unwrap();
        assert!(evm_at < utxo_at, "largest store must sort first:\n{out}");

        // Empty stores are omitted from the table, and the summary says how many.
        assert!(!out.contains("EvmFlatAccount"), "empty stores should not pad the table:\n{out}");
        assert!(out.contains("2 of 3 registered stores hold data."), "{out}");

        // The actionable note, not just a number.
        assert!(out.contains("--evm-storage-profile=compact"), "{out}");
        assert!(out.contains("pending compaction"), "{out}");
    }

    #[test]
    fn report_omits_the_206_note_when_the_store_is_actually_retired() {
        let stats = DbSizeStats {
            // A retired store can still show a non-zero byte ESTIMATE until compaction
            // catches up; emptiness is the signal, so the note must key off that.
            prefixes: vec![prefix(206, "EvmStateDiff", 4 * 1024 * 1024 * 1024, false, Some(0))],
            ..Default::default()
        };
        let out = render(Path::new("/var/lib/misaka/consensus"), &stats, true);
        assert!(!out.contains("--evm-storage-profile=compact"), "{out}");
        // With rows counted, the hint to re-run for exact numbers is pointless.
        assert!(!out.contains("full scan"), "{out}");
    }

    #[test]
    fn uncounted_rows_report_emptiness_rather_than_a_misleading_zero() {
        let stats = DbSizeStats {
            prefixes: vec![prefix(206, "EvmStateDiff", 1024, true, None), prefix(25, "UtxoDiffs", 2048, false, None)],
            ..Default::default()
        };
        let out = render(Path::new("/tmp/consensus"), &stats, false);
        assert!(out.contains("non-empty"), "{out}");
        assert!(out.contains("empty"), "{out}");
        assert!(out.contains("--db-stats=count-rows"), "{out}");
    }
}
