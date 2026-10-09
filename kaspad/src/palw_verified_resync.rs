//! **LIVE-R1: `kaspad --palw-verified-resync`** — the operator's remedy for a node the partition watchdog
//! held (`kaspa_p2p_flows::flowcontext::partition_watch`), and for any node whose own branch the network
//! has left.
//!
//! Why a resync at all: on testnet-12 the PALW deep-reorg rule refuses a heavier chain that does not
//! strictly out-weigh this node's own, so a minority that carried a licence the majority did not keeps its
//! branch; once that branch is `finality_depth` above the fork, nothing in consensus can move it. And an
//! IBD onto the EXISTING data directory is refused by the same rule at the pruning-proof commit
//! (`palw_pruning_proof_strict_economic_win`). Only an empty data directory has no incumbent to defend.
//!
//! What this does, in order:
//! 1. **Moves the data directory aside** — renamed to `datadir.pre-resync-<unix ms>` next to it, never
//!    deleted — and writes `palw-resync-report.json` (`status: syncing`, where the old one went). A
//!    failure to move it refuses to start rather than syncing over it.
//! 2. **Syncs from empty**, by the ordinary IBD: the pruning proof is validated, every body after the
//!    pruning point is validated, and the PALW fold is computed by this node — nothing is taken on a
//!    peer's word.
//! 3. **Stays held** (no mining, no attesting, `is_synced` false) until at least two long-lived OUTBOUND
//!    peers relay blocks extending the synced chain, they are a strict majority of the long-lived
//!    outbound peers, and the synced chain refuses nothing heavier on the PALW rule. Then the report is
//!    rewritten (`status: confirmed`, the sink, its DAA, the peer counts) and the node participates.
//!
//! Threat model. It is never started by the node itself, so no input an attacker controls starts it.
//! A node an attacker eclipses during the sync can be handed the attacker's branch by its IBD peer; it
//! then stays held, because its long-lived outbound peers — chosen by this node — relay another chain,
//! and inbound connections, however many, confirm nothing. The old data directory is beside it either way.
use std::path::{Path, PathBuf};

pub const REPORT_FILE: &str = "palw-resync-report.json";

/// Step 1: move `db_dir` aside and write the `syncing` report. Returns where it went (`None` when there was
/// nothing to move: a fresh directory needs no aside, and the resync is then just a verified first sync).
pub fn move_datadir_aside(db_dir: &Path, report_path: &Path) -> Result<Option<PathBuf>, String> {
    let now = kaspa_core::time::unix_now();
    let aside = if db_dir.exists() && db_dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false) {
        let name = db_dir.file_name().and_then(|n| n.to_str()).unwrap_or("datadir");
        let aside = db_dir.with_file_name(format!("{name}.pre-resync-{now}"));
        if aside.exists() {
            return Err(format!("{} already exists; refusing to overwrite it", aside.display()));
        }
        std::fs::rename(db_dir, &aside).map_err(|e| {
            format!("could not move {} aside to {}: {e} — refusing to sync over it", db_dir.display(), aside.display())
        })?;
        Some(aside)
    } else {
        None
    };
    let report = serde_json::json!({
        "status": "syncing",
        "startedUnixMs": now,
        "dataDirectory": db_dir.display().to_string(),
        "previousDataDirectoryMovedTo": aside.as_ref().map(|p| p.display().to_string()),
        "confirmation": format!(
            "held (no mining, no attesting, unsynced) until at least {} long-lived outbound peers relay blocks extending the synced chain",
            kaspa_p2p_flows::flowcontext::partition_watch::RESYNC_MIN_CONFIRMING_PEERS
        ),
    });
    write_report(report_path, &report)?;
    kaspa_core::warn!(
        "VERIFIED RESYNC: data directory {} {}; report {}",
        db_dir.display(),
        match &aside {
            Some(a) => format!("moved aside to {} (kept — delete it yourself once the report says confirmed)", a.display()),
            None => "was empty — nothing to move".to_owned(),
        },
        report_path.display()
    );
    Ok(aside)
}

/// The reporter the flow context calls once the synced chain is confirmed: rewrites the report.
pub fn confirmed_reporter(report_path: PathBuf, aside: Option<PathBuf>) -> kaspa_p2p_flows::flow_context::ResyncReporter {
    std::sync::Arc::new(move |r: &kaspa_p2p_flows::flow_context::ResyncReport| {
        let report = serde_json::json!({
            "status": "confirmed",
            "confirmedUnixMs": r.confirmed_unix_ms,
            "sink": r.sink.to_string(),
            "sinkDaa": r.sink_daa,
            "confirmingOutboundPeers": r.confirming_outbound_peers,
            "eligibleOutboundPeers": r.eligible_outbound_peers,
            "previousDataDirectoryMovedTo": aside.as_ref().map(|p| p.display().to_string()),
        });
        if let Err(e) = write_report(&report_path, &report) {
            kaspa_core::warn!("VERIFIED RESYNC: confirmed, but the report could not be written: {e}");
        }
    })
}

fn write_report(path: &Path, report: &serde_json::Value) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(report).expect("a json value serializes"))
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The data directory is moved, not deleted; the report says where; a second resync in the same
    /// millisecond cannot overwrite the first's aside; an empty directory has nothing to move.
    #[test]
    fn live_r1_the_resync_moves_the_datadir_aside_and_reports_it() {
        let root = std::env::temp_dir().join(format!("live-r1-resync-{}", kaspa_core::time::unix_now()));
        let db = root.join("datadir");
        std::fs::create_dir_all(db.join("consensus")).unwrap();
        std::fs::write(db.join("consensus").join("MARKER"), b"old chain").unwrap();
        let report = root.join(REPORT_FILE);
        let aside = move_datadir_aside(&db, &report).unwrap().expect("a non-empty datadir is moved");
        assert!(!db.exists(), "the node will sync into an empty directory");
        assert_eq!(std::fs::read(aside.join("consensus").join("MARKER")).unwrap(), b"old chain", "kept intact");
        let written: serde_json::Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        assert_eq!(written["status"], "syncing");
        assert_eq!(written["previousDataDirectoryMovedTo"], aside.display().to_string());

        confirmed_reporter(report.clone(), Some(aside.clone()))(&kaspa_p2p_flows::flow_context::ResyncReport {
            confirmed_unix_ms: 1,
            sink: kaspa_consensus_core::BlockHash::from_u64_word(7),
            sink_daa: 101,
            confirming_outbound_peers: 6,
            eligible_outbound_peers: 8,
        });
        let written: serde_json::Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        assert_eq!((written["status"].as_str(), written["sinkDaa"].as_u64()), (Some("confirmed"), Some(101)));
        assert!(aside.exists(), "confirmation does not delete the old directory either");

        std::fs::create_dir_all(&db).unwrap();
        assert_eq!(move_datadir_aside(&db, &report).unwrap(), None, "an empty directory has nothing to move");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
