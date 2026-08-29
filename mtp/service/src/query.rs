//! Read-only self-serve query views (ADR-0038 D3, I-MTP-10).
//!
//! **The authoritative artifact is the published, ML-DSA-87-signed epoch ledger —
//! not any response here.** Every number a [`PointsView`] carries is copied
//! verbatim from a signed ledger file in the [`LedgerArchive`]; this module never
//! recomputes a score, so a participant can always fall back to
//! `misaka mtp verify-epoch` on the same signed file to check it independently.
//!
//! Cumulative totals sum **only the latest issue per epoch**, so a supersede
//! (D6) never double-counts: the old issue's numbers are visible for audit but do
//! not contribute to the running total.

use crate::publish::{ArchiveError, LedgerArchive};
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum QueryError {
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    #[error("no such id in any published epoch")]
    UnknownId,
    #[error("no published ledger for epoch {0}")]
    UnknownEpoch(u64),
    #[error("epoch {0} was published ledger-only (no facts sidecar to recompute from)")]
    NoFacts(u64),
}

/// Per-category cumulative totals for an id (milli-points), plus the grand total.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cumulative {
    pub c1: u64,
    pub c2: u64,
    pub c3: u64,
    pub c4: u64,
    /// C5 LLM mining (ADR-0040 §16″). `serde` default so a client reading a pre-C5 response still
    /// deserializes.
    #[serde(default)]
    pub c5: u64,
    pub total: u64,
}

/// One epoch's contribution to an id (D3), copied from the latest signed issue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochView {
    pub epoch: u64,
    /// The issue this row was read from (the latest issue for the epoch).
    pub issue: u32,
    pub network: String,
    pub c1: u64,
    pub c2: u64,
    pub c3: u64,
    pub c4: u64,
    /// C5 LLM mining. Provisional while `c5_token_settlement_enabled()` is false — the points are
    /// measured and signed, the token value they carry is not decided.
    #[serde(default)]
    pub c5: u64,
    pub evidence: Vec<String>,
    pub rules_hash: String,
    pub inputs_hash: String,
    /// `true` when this epoch has been reissued at least once (older, superseded
    /// issues exist to inspect via `GET /mtp/v1/epoch/<n>`). The row itself is
    /// always the current one.
    pub superseded: bool,
    /// The signed JSONL file this row was read from (the D3 provenance link).
    pub file: String,
}

/// The self-serve points view for one id (D3): cumulative totals + per-epoch rows,
/// each traceable to the signed file it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointsView {
    pub id: String,
    pub cumulative: Cumulative,
    pub epochs: Vec<EpochView>,
    pub latest_epoch: Option<u64>,
}

/// Assemble the [`PointsView`] for `id` from the archive. For each epoch that has
/// a published ledger, read the **latest** signed issue, find the id's score row
/// (if any), and accumulate. An id that never appears in any latest issue is
/// [`QueryError::UnknownId`].
pub fn points_view(archive: &LedgerArchive, id: &str) -> Result<PointsView, QueryError> {
    let mut epochs = Vec::new();
    let mut cum = Cumulative::default();
    let mut latest_epoch = None;

    for epoch in archive.epochs() {
        let latest = archive.latest(epoch).expect("epochs() only lists epochs with an issue");
        let ledger = archive.read_ledger(epoch, latest.issue)?;
        let Some(row) = ledger.scores.iter().find(|s| s.id == id) else {
            continue;
        };
        cum.c1 += row.c1;
        cum.c2 += row.c2;
        cum.c3 += row.c3;
        cum.c4 += row.c4;
        cum.c5 += row.c5;
        latest_epoch = Some(latest_epoch.map_or(epoch, |cur: u64| cur.max(epoch)));
        epochs.push(EpochView {
            epoch,
            issue: latest.issue,
            network: ledger.network.clone(),
            c1: row.c1,
            c2: row.c2,
            c3: row.c3,
            c4: row.c4,
            c5: row.c5,
            evidence: row.evidence.clone(),
            rules_hash: ledger.rules_hash.clone(),
            inputs_hash: ledger.inputs_hash.clone(),
            superseded: latest.issue > 0,
            file: latest.file.clone(),
        });
    }

    if epochs.is_empty() {
        return Err(QueryError::UnknownId);
    }
    // C5 belongs in the grand total: it is a scored category like any other. Leaving it out
    // made the ledger and the self-serve view disagree about what an id had earned.
    cum.total = cum.c1 + cum.c2 + cum.c3 + cum.c4 + cum.c5;
    Ok(PointsView { id: id.to_string(), cumulative: cum, epochs, latest_epoch })
}

/// One row of the [`Leaderboard`]: an id with its cumulative totals and 1-based rank.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    /// 1-based position (by grand total, ties broken by id).
    pub rank: u64,
    pub id: String,
    pub cumulative: Cumulative,
}

/// The full points leaderboard — every id that appears in any epoch's latest signed
/// issue, with its cumulative totals. Public, testnet-only, and derived purely from the
/// signed archive, so it is exactly the aggregate mirror of each [`points_view`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leaderboard {
    /// The scored network (from the ledgers), e.g. `"testnet-palw-10"`.
    pub network: String,
    /// How many epochs contributed (one latest issue each).
    pub epochs_counted: u64,
    /// The highest epoch counted, if any.
    pub latest_epoch: Option<u64>,
    /// Number of distinct ids on the board.
    pub participants: u64,
    /// Rows, highest total first.
    pub entries: Vec<LeaderboardEntry>,
}

/// Assemble the full [`Leaderboard`] across every published epoch. For each epoch it
/// reads the **latest** signed issue (a supersede replaces, never double-counts — the
/// same rule [`points_view`] uses) and folds every score row into a per-id total, then
/// ranks by grand total descending (ties broken by id, so the order is deterministic).
/// An empty archive yields an empty board (not an error).
pub fn leaderboard(archive: &LedgerArchive) -> Result<Leaderboard, QueryError> {
    let mut totals: std::collections::BTreeMap<String, Cumulative> = std::collections::BTreeMap::new();
    let mut epochs_counted = 0u64;
    let mut latest_epoch: Option<u64> = None;
    let mut network = String::new();

    for epoch in archive.epochs() {
        let latest = archive.latest(epoch).expect("epochs() only lists epochs with an issue");
        let ledger = archive.read_ledger(epoch, latest.issue)?;
        epochs_counted += 1;
        latest_epoch = Some(latest_epoch.map_or(epoch, |cur: u64| cur.max(epoch)));
        if network.is_empty() {
            network = ledger.network.clone();
        }
        for row in &ledger.scores {
            let e = totals.entry(row.id.clone()).or_default();
            e.c1 += row.c1;
            e.c2 += row.c2;
            e.c3 += row.c3;
            e.c4 += row.c4;
        }
    }

    let mut ranked: Vec<(String, Cumulative)> = totals
        .into_iter()
        .map(|(id, mut c)| {
            c.total = c.c1 + c.c2 + c.c3 + c.c4;
            (id, c)
        })
        .collect();
    // Highest total first; ties broken by id ascending for a stable, deterministic board.
    ranked.sort_by(|a, b| b.1.total.cmp(&a.1.total).then_with(|| a.0.cmp(&b.0)));

    let participants = ranked.len() as u64;
    let entries =
        ranked.into_iter().enumerate().map(|(i, (id, cumulative))| LeaderboardEntry { rank: i as u64 + 1, id, cumulative }).collect();
    Ok(Leaderboard { network, epochs_counted, latest_epoch, participants, entries })
}

/// The signed JSONL of an epoch's latest issue, byte-exact (`GET /mtp/v1/epoch/<n>`).
pub fn epoch_jsonl(archive: &LedgerArchive, epoch: u64) -> Result<String, QueryError> {
    let latest = archive.latest(epoch).ok_or(QueryError::UnknownEpoch(epoch))?;
    Ok(archive.read_jsonl(epoch, latest.issue)?)
}

/// The published `EpochInput` facts JSON for an epoch's latest issue (the D3
/// recompute source, `GET /mtp/v1/epoch/<n>/facts`). Verbatim, byte-exact.
pub fn epoch_facts_jsonl(archive: &LedgerArchive, epoch: u64) -> Result<String, QueryError> {
    let latest = archive.latest(epoch).ok_or(QueryError::UnknownEpoch(epoch))?;
    archive.read_input_json(epoch, latest.issue)?.ok_or(QueryError::NoFacts(epoch))
}

/// Every issue's signed JSONL for `epoch`, latest first (the "all issues if
/// superseded" form of `GET /mtp/v1/epoch/<n>`).
pub fn epoch_all_issues_jsonl(archive: &LedgerArchive, epoch: u64) -> Result<Vec<String>, QueryError> {
    let issues = archive.issues(epoch);
    if issues.is_empty() {
        return Err(QueryError::UnknownEpoch(epoch));
    }
    let mut out = Vec::with_capacity(issues.len());
    for e in issues {
        out.push(archive.read_jsonl(epoch, e.issue)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_pq_validator_core::ValidatorKey;
    use misaka_mtp::{EpochLedger, ScoreRow};
    use std::path::PathBuf;

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let uniq = format!("mtp-query-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed));
        let p = std::env::temp_dir().join(uniq);
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn ledger(epoch: u64, rows: &[(&str, [u64; 5])], key: &ValidatorKey) -> EpochLedger {
        let scores = rows
            .iter()
            .map(|(id, p)| ScoreRow {
                id: (*id).into(),
                c1: p[0],
                c2: p[1],
                c3: p[2],
                c4: p[3],
                c5: p[4],
                evidence: vec![format!("ev-{id}")],
            })
            .collect();
        let mut l = EpochLedger {
            epoch,
            range: ["s".into(), "e".into()],
            network: "testnet-10".into(),
            rules_hash: "aa".into(),
            inputs_hash: "bb".into(),
            scores,
            sig_mldsa87: None,
        };
        l.sign(key);
        l
    }

    #[test]
    fn points_view_accumulates_across_epochs() {
        let dir = tempdir();
        let key = ValidatorKey::from_seed([9; 32]);
        let mut a = LedgerArchive::open(&dir).unwrap();
        a.publish(&ledger(1, &[("gh:alice", [100, 0, 0, 0, 0]), ("gh:bob", [0, 50, 0, 0, 0])], &key), "", "").unwrap();
        a.publish(&ledger(2, &[("gh:alice", [0, 0, 30, 0, 0])], &key), "", "").unwrap();

        let v = points_view(&a, "gh:alice").unwrap();
        assert_eq!(v.cumulative, Cumulative { c1: 100, c2: 0, c3: 30, c4: 0, c5: 0, total: 130 });
        assert_eq!(v.epochs.len(), 2);
        assert_eq!(v.latest_epoch, Some(2));
        // bob only appears in epoch 1.
        let vb = points_view(&a, "gh:bob").unwrap();
        assert_eq!(vb.cumulative.total, 50);
        assert_eq!(vb.epochs.len(), 1);
        // an unregistered id → UnknownId.
        assert!(matches!(points_view(&a, "gh:nobody"), Err(QueryError::UnknownId)));
    }

    #[test]
    fn leaderboard_ranks_all_ids_by_total_latest_issue_only() {
        let dir = tempdir();
        let key = ValidatorKey::from_seed([9; 32]);
        let mut a = LedgerArchive::open(&dir).unwrap();
        a.publish(&ledger(1, &[("gh:alice", [100, 0, 0, 0, 0]), ("gh:bob", [0, 50, 0, 0, 0])], &key), "", "").unwrap();
        a.publish(&ledger(2, &[("gh:alice", [0, 0, 30, 0, 0]), ("gh:carol", [200, 0, 0, 0, 0])], &key), "", "").unwrap();
        // Supersede epoch 1 (bob 50 → 70): the board must use the LATEST issue only (bob 70, not 120).
        a.publish(&ledger(1, &[("gh:alice", [100, 0, 0, 0, 0]), ("gh:bob", [0, 70, 0, 0, 0])], &key), "appeal", "url").unwrap();

        let lb = leaderboard(&a).unwrap();
        assert_eq!(lb.participants, 3);
        assert_eq!(lb.epochs_counted, 2);
        assert_eq!(lb.latest_epoch, Some(2));
        // Ranked by grand total descending: carol 200, alice 130, bob 70.
        let got: Vec<(u64, &str, u64)> = lb.entries.iter().map(|e| (e.rank, e.id.as_str(), e.cumulative.total)).collect();
        assert_eq!(got, vec![(1, "gh:carol", 200), (2, "gh:alice", 130), (3, "gh:bob", 70)]);
        // Alice's per-category breakdown sums the latest issue of each epoch she appears in.
        assert_eq!(lb.entries[1].cumulative, Cumulative { c1: 100, c2: 0, c3: 30, c4: 0, c5: 0, total: 130 });
    }

    #[test]
    fn leaderboard_empty_archive_is_empty_not_error() {
        let dir = tempdir();
        let a = LedgerArchive::open(&dir).unwrap();
        let lb = leaderboard(&a).unwrap();
        assert!(lb.entries.is_empty());
        assert_eq!((lb.participants, lb.epochs_counted, lb.latest_epoch), (0, 0, None));
    }

    #[test]
    fn cumulative_uses_only_the_latest_issue_no_double_count() {
        let dir = tempdir();
        let key = ValidatorKey::from_seed([8; 32]);
        let mut a = LedgerArchive::open(&dir).unwrap();
        a.publish(&ledger(1, &[("gh:alice", [100, 0, 0, 0, 0])], &key), "", "").unwrap();
        // correction: alice's C1 was undercounted → reissue with 150.
        a.publish(&ledger(1, &[("gh:alice", [150, 0, 0, 0, 0])], &key), "appeal #3", "url").unwrap();

        let v = points_view(&a, "gh:alice").unwrap();
        // only the latest issue counts — 150, not 100+150.
        assert_eq!(v.cumulative.c1, 150);
        assert_eq!(v.epochs.len(), 1);
        assert_eq!(v.epochs[0].issue, 1);
        assert!(v.epochs[0].superseded, "epoch flags that a correction exists");

        // the raw mirror still exposes BOTH issues for audit.
        let all = epoch_all_issues_jsonl(&a, 1).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].contains("150000") || all[0].contains("\"c1\":150"));
    }

    #[test]
    fn epoch_jsonl_is_the_latest_signed_file_verbatim() {
        let dir = tempdir();
        let key = ValidatorKey::from_seed([7; 32]);
        let mut a = LedgerArchive::open(&dir).unwrap();
        let l = ledger(4, &[("gh:alice", [1, 2, 3, 4, 0])], &key);
        a.publish(&l, "", "").unwrap();

        let raw = epoch_jsonl(&a, 4).unwrap();
        // the served bytes parse back to a ledger whose signature verifies.
        let parsed: EpochLedger = serde_json::from_str(raw.trim_end()).unwrap();
        assert_eq!(parsed, l);
        assert!(parsed.verify(key.public_key()));
        assert!(matches!(epoch_jsonl(&a, 99), Err(QueryError::UnknownEpoch(99))));
    }
}
