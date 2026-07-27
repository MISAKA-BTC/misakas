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
/// EVM-lane diagnostics (read-only, `--features evm`). `payloads` attributes the
/// 211 EvmPayload bytes to their fields; `skips` gives the 204 acceptance/skip
/// distribution; `class2` decodes never-accepted txs and pins the class-2
/// sub-cause (nonce/basefee/funds) + source-address concentration.
pub const MODE_PAYLOADS: &str = "payloads";
pub const MODE_SKIPS: &str = "skips";
pub const MODE_CLASS2: &str = "class2";

/// How many EvmPayload rows the `payloads` mode samples (each decodes ~256 txs).
const PAYLOADS_SAMPLE: usize = 50_000;

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
        // EVM-lane diagnostics (read-only): attribute/classify rather than size.
        match mode {
            MODE_PAYLOADS => {
                dump_evm_payloads(&db, db_path, PAYLOADS_SAMPLE);
                continue;
            }
            MODE_SKIPS => {
                dump_evm_skips(&db, db_path, usize::MAX);
                continue;
            }
            MODE_CLASS2 => {
                dump_evm_class2(&db, db_path);
                continue;
            }
            _ => {}
        }
        print!("{}", render(db_path, &size_stats::collect(&db, count_rows), count_rows));
        println!();
    }
    if failures == databases.len() { 1 } else { 0 }
}

/// Attribute prefix-211 (`EvmPayload`) value bytes to the `EvmExecutionPayload`
/// fields (transactions / system_ops / extra_data). Read-only. Diagnostic for the
/// "20 KiB per payload on an idle chain" question.
#[cfg(feature = "evm")]
fn dump_evm_payloads(db: &kaspa_database::prelude::DB, db_path: &Path, limit: usize) {
    use kaspa_consensus_core::evm::EvmExecutionPayload;
    let prefix = kaspa_database::registry::DatabaseStorePrefixes::EvmPayload as u8;
    println!("EvmPayload (211) field attribution — {}\n", db_path.display());
    let (mut scanned, mut total, mut maxv) = (0u64, 0u64, 0u64);
    let (mut tx_bytes, mut tx_count, mut sysops_bytes, mut sysops_count, mut extra_bytes) = (0u64, 0u64, 0u64, 0u64, 0u64);
    // Distinct tx CONTENT across the sample: if this is far below tx_count, the
    // same txs are being re-included in payload after payload (the redundancy
    // hypothesis) rather than the chain carrying diverse traffic.
    let mut distinct: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut shown = 0;
    for item in db.iterator(rocksdb::IteratorMode::From(&[prefix], rocksdb::Direction::Forward)) {
        let Ok((key, val)) = item else { break };
        if key.first() != Some(&prefix) {
            break;
        }
        scanned += 1;
        total += val.len() as u64;
        maxv = maxv.max(val.len() as u64);
        if let Ok(p) = bincode::deserialize::<EvmExecutionPayload>(&val) {
            let t: u64 = p.transactions.iter().map(|x| x.len() as u64).sum();
            for tx in &p.transactions {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                tx.hash(&mut h);
                distinct.insert(h.finish());
            }
            let s = bincode::serialized_size(&p.system_ops).unwrap_or(0);
            tx_bytes += t;
            tx_count += p.transactions.len() as u64;
            sysops_bytes += s;
            sysops_count += p.system_ops.len() as u64;
            extra_bytes += p.extra_data.len() as u64;
            if shown < 3 {
                shown += 1;
                println!(
                    "  sample[{shown}] value={}B  transactions={} (={}B)  system_ops={} (={}B)  extra_data={}B",
                    val.len(),
                    p.transactions.len(),
                    t,
                    p.system_ops.len(),
                    s,
                    p.extra_data.len()
                );
                if let Some(tx0) = p.transactions.first() {
                    let head: Vec<u8> = tx0.iter().take(12).copied().collect();
                    println!("            tx[0]={}B head={:02x?}", tx0.len(), head);
                }
                if !p.extra_data.is_empty() {
                    let head: Vec<u8> = p.extra_data.iter().take(24).copied().collect();
                    println!("            extra_data head={:02x?}", head);
                }
            }
        }
        if scanned >= limit as u64 {
            break;
        }
    }
    if scanned == 0 {
        println!("  no EvmPayload rows.");
        return;
    }
    println!("\n  scanned {scanned} rows: mean value={}B  max={}B", total / scanned, maxv);
    println!(
        "  mean/payload  transactions={:.2} (={}B)  system_ops={:.2} (={}B)  extra_data={}B",
        tx_count as f64 / scanned as f64,
        tx_bytes / scanned,
        sysops_count as f64 / scanned as f64,
        sysops_bytes / scanned,
        extra_bytes / scanned
    );
    let d = distinct.len() as u64;
    println!(
        "  tx redundancy: {} total inclusions vs {} DISTINCT tx contents  => each distinct tx re-included ~{}x",
        tx_count,
        d,
        if d > 0 { tx_count / d } else { 0 }
    );
}

#[cfg(not(feature = "evm"))]
fn dump_evm_payloads(_db: &kaspa_database::prelude::DB, _db_path: &Path, _limit: usize) {
    println!("MISAKA_DUMP_EVM_PAYLOADS requires an --features evm build.");
}

/// Acceptance/skip distribution over prefix 204 (`EvmTxLookup`). For each tx:
/// was it ever accepted (`accepted_in` non-empty), and if never, which §6.1 skip
/// class was last recorded (2 = nonce/funds/basefee, 3 = duplicate, 5 = gas-cap,
/// 1 = undecodable). This is the direct read of "why is accepted-gas 0%".
#[cfg(feature = "evm")]
fn dump_evm_skips(db: &kaspa_database::prelude::DB, db_path: &Path, limit: usize) {
    use kaspa_consensus_core::evm::EvmTxLocations;
    let prefix = kaspa_database::registry::DatabaseStorePrefixes::EvmTxLookup as u8;
    println!("EvmTxLookup (204) acceptance/skip attribution — {}\n", db_path.display());
    let (mut scanned, mut accepted, mut never, mut inc_total) = (0u64, 0u64, 0u64, 0u64);
    let mut class = [0u64; 8]; // index 1/2/3/5 used; 0 = "never-accepted but last_skip_class None"
    for item in db.iterator(rocksdb::IteratorMode::From(&[prefix], rocksdb::Direction::Forward)) {
        let Ok((key, val)) = item else { break };
        if key.first() != Some(&prefix) {
            break;
        }
        scanned += 1;
        if let Ok(loc) = bincode::deserialize::<EvmTxLocations>(&val) {
            inc_total += loc.included_in.len() as u64;
            if !loc.accepted_in.is_empty() {
                accepted += 1;
            } else {
                never += 1;
                let c = loc.last_skip_class.unwrap_or(0) as usize;
                class[c.min(7)] += 1;
            }
        }
        if scanned >= limit as u64 {
            break;
        }
    }
    if scanned == 0 {
        println!("  no EvmTxLookup rows.");
        return;
    }
    println!("  scanned {scanned} distinct txs:  accepted={accepted}  never_accepted={never}");
    println!("  acceptance rate = {:.4}%", accepted as f64 * 100.0 / scanned as f64);
    println!(
        "  mean included_in (payload blocks/tx, capped at MAX_TX_LOCATION_INCLUSIONS) = {:.2}",
        inc_total as f64 / scanned as f64
    );
    println!(
        "  never-accepted last_skip_class:  class1(undecodable)={}  class2(nonce/funds/basefee)={}  class3(dup)={}  class5(gas-cap)={}  none={}",
        class[1], class[2], class[3], class[5], class[0]
    );
}

#[cfg(not(feature = "evm"))]
fn dump_evm_skips(_db: &kaspa_database::prelude::DB, _db_path: &Path, _limit: usize) {
    println!("MISAKA_DUMP_EVM_SKIPS requires an --features evm build.");
}

/// Decode the never-accepted active txs (prefix 217) and classify the class-2
/// sub-cause against this node's flat state (234) + acceptance index (204).
/// revm rejects in order nonce → basefee → funds, so a stuck sender's HEAD tx
/// (nonce == state nonce) carries the real reason; its higher-nonce successors
/// are merely blocked behind it (counted `nonce_ahead`).
#[cfg(feature = "evm")]
fn dump_evm_class2(db: &kaspa_database::prelude::DB, db_path: &Path) {
    use kaspa_consensus_core::evm::{EVM_INITIAL_BASE_FEE, EvmRawTx, EvmTxLocations, FlatAccount};
    use kaspa_database::registry::DatabaseStorePrefixes as P;
    use std::collections::HashMap;
    let it = |p: u8| db.iterator(rocksdb::IteratorMode::From(&[p], rocksdb::Direction::Forward));

    // 204: tx hash -> ever accepted?
    let mut accepted: HashMap<[u8; 32], bool> = HashMap::new();
    for item in it(P::EvmTxLookup as u8) {
        let Ok((k, v)) = item else { break };
        if k.first() != Some(&(P::EvmTxLookup as u8)) {
            break;
        }
        if k.len() >= 33
            && let Ok(loc) = bincode::deserialize::<EvmTxLocations>(&v)
        {
            let mut h = [0u8; 32];
            h.copy_from_slice(&k[1..33]);
            accepted.insert(h, !loc.accepted_in.is_empty());
        }
    }
    // 234: address -> (state nonce, balance)
    let mut acct: HashMap<[u8; 20], (u64, u128)> = HashMap::new();
    for item in it(P::EvmFlatAccount as u8) {
        let Ok((k, v)) = item else { break };
        if k.first() != Some(&(P::EvmFlatAccount as u8)) {
            break;
        }
        if k.len() >= 21
            && let Ok(a) = bincode::deserialize::<FlatAccount>(&v)
        {
            let mut addr = [0u8; 20];
            addr.copy_from_slice(&k[1..21]);
            acct.insert(addr, (a.core.nonce, a.core.balance.try_to_u128().unwrap_or(u128::MAX)));
        }
    }
    let base_fee = EVM_INITIAL_BASE_FEE as u128;
    println!("class-2 sub-cause over prefix 217 (active unique set) — {}", db_path.display());
    println!("  base_fee={base_fee}  accounts_in_flat_state={}\n", acct.len());

    let (mut total, mut acc, mut never, mut derr) = (0u64, 0u64, 0u64, 0u64);
    let (mut nonce_ahead, mut nonce_behind, mut basefee, mut funds, mut would, mut sender_absent) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut na_sender_absent, mut na_balance_zero, mut na_funded, mut na_gap_max, mut na_gap_sum) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut shown, mut na_shown) = (0, 0);
    // Per-sender aggregation: (stuck, accepted, state_nonce, balance, min_stuck_nonce, max_stuck_nonce).
    let mut by_sender: HashMap<[u8; 20], (u64, u64, u64, u128, u64, u64)> = HashMap::new();
    for item in it(P::EvmRawTransaction as u8) {
        let Ok((k, v)) = item else { break };
        if k.first() != Some(&(P::EvmRawTransaction as u8)) {
            break;
        }
        if k.len() < 33 {
            continue;
        }
        total += 1;
        let mut h = [0u8; 32];
        h.copy_from_slice(&k[1..33]);
        let is_acc = accepted.get(&h).copied().unwrap_or(false);
        let raw = match bincode::deserialize::<EvmRawTx>(&v) {
            Ok(r) => r.raw,
            Err(_) => {
                derr += 1;
                continue;
            }
        };
        let d = match kaspa_evm::tx::decode_eth_tx(&raw) {
            Ok(d) => d,
            Err(_) => {
                derr += 1;
                continue;
            }
        };
        let has_acct = acct.contains_key(&d.from);
        let (state_nonce, balance) = acct.get(&d.from).copied().unwrap_or((0, 0));
        {
            let e = by_sender.entry(d.from).or_insert((0, 0, state_nonce, balance, u64::MAX, 0));
            e.2 = state_nonce;
            e.3 = balance;
            if is_acc {
                e.1 += 1;
            } else {
                e.0 += 1;
                e.4 = e.4.min(d.nonce);
                e.5 = e.5.max(d.nonce);
            }
        }
        if is_acc {
            acc += 1;
            continue;
        }
        never += 1;
        // value [u8;32] big-endian → u128 (saturate if the high 128 bits are set).
        let value =
            if d.value[..16].iter().any(|&b| b != 0) { u128::MAX } else { u128::from_be_bytes(d.value[16..32].try_into().unwrap()) };
        if d.nonce > state_nonce {
            nonce_ahead += 1;
            if !has_acct {
                na_sender_absent += 1;
            } else if balance == 0 {
                na_balance_zero += 1;
            } else {
                na_funded += 1;
            }
            let gap = d.nonce - state_nonce;
            na_gap_max = na_gap_max.max(gap);
            na_gap_sum += gap;
            if na_shown < 3 {
                na_shown += 1;
                println!(
                    "  nonce_ahead sample[{na_shown}] from=0x{}  tx_nonce={}  state_nonce={}  gap={}  balance={}  in_flat_state={}",
                    hex20(&d.from),
                    d.nonce,
                    state_nonce,
                    gap,
                    balance,
                    has_acct
                );
            }
        } else if d.nonce < state_nonce {
            nonce_behind += 1;
        } else if d.max_fee_per_gas < base_fee {
            basefee += 1;
        } else {
            let need = (d.gas_limit as u128).saturating_mul(d.max_fee_per_gas).saturating_add(value);
            if balance < need {
                funds += 1;
                if !has_acct {
                    sender_absent += 1;
                }
                if shown < 3 {
                    shown += 1;
                    println!(
                        "  funds sample[{shown}] from=0x{}  nonce={}  gas_limit={}  max_fee={}  value={}  balance={}  need={}",
                        hex20(&d.from),
                        d.nonce,
                        d.gas_limit,
                        d.max_fee_per_gas,
                        value,
                        balance,
                        need
                    );
                }
            } else {
                would += 1;
            }
        }
    }
    println!("\n  217 rows: total={total} accepted={acc} never_accepted={never} decode_err={derr}");
    println!(
        "  HEAD-tx reason (nonce == state nonce):  basefee_low={basefee}  insufficient_funds={funds} (sender-absent={sender_absent})  would_accept={would}"
    );
    println!("  blocked behind a stuck head:  nonce_ahead(gap)={nonce_ahead}  nonce_behind(stale)={nonce_behind}");
    if nonce_ahead > 0 {
        println!(
            "  nonce_ahead senders:  absent_from_flat_state={na_sender_absent}  balance_zero={na_balance_zero}  funded={na_funded}  |  gap mean={:.1} max={na_gap_max}",
            na_gap_sum as f64 / nonce_ahead as f64
        );
    }
    // Source concentration: top senders by stuck-tx count.
    let mut v: Vec<_> = by_sender.into_iter().collect();
    v.sort_by(|a, b| b.1.0.cmp(&a.1.0));
    let with_stuck = v.iter().filter(|(_, x)| x.0 > 0).count();
    println!("\n  distinct senders with >=1 stuck tx: {with_stuck}");
    println!("  top senders by stuck count (stuck / accepted / state_nonce / balance / stuck-nonce range):");
    for (addr, (stuck, acc_n, sn, bal, mn, mx)) in v.iter().take(12).filter(|(_, x)| x.0 > 0) {
        println!(
            "    0x{}  stuck={stuck}  accepted={acc_n}  state_nonce={sn}  balance={bal}  nonce=[{}..{mx}]",
            hex20(addr),
            if *mn == u64::MAX { 0 } else { *mn }
        );
    }
}

#[cfg(feature = "evm")]
fn hex20(b: &[u8; 20]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(not(feature = "evm"))]
fn dump_evm_class2(_db: &kaspa_database::prelude::DB, _db_path: &Path) {
    println!("MISAKA_DUMP_EVM_CLASS2 requires an --features evm build.");
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
