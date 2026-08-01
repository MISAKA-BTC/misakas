//! Persistent, **time-stamped** fact store (ADR-0038 D2, G3/I-MTP-3).
//!
//! The collectors crate's [`misaka_mtp_collectors::FactStore`] is an in-memory
//! `Vec`-per-table snapshot whose documented contract is *"must hold facts for
//! exactly one epoch"* — `build_epoch_input` counts every row it sees. A running
//! service accumulates facts across many epochs, so it needs a durable store that
//! remembers **when** each fact happened and can hand the cron a *fresh*
//! single-epoch `FactStore` on demand. That is exactly what this module is: the
//! same §4.2 schema, every row tagged with an activity timestamp, persisted as
//! append-only JSONL, with [`PersistentStore::window`] projecting one epoch.
//!
//! ## SQLite equivalence (D2 refinement)
//! ADR-0038 D2 names SQLite for this store. The workspace pins tokio 1.42.1 and
//! carries no `rusqlite`; rather than pull a C-linked dependency for a testnet-
//! scale table set, this is a pure-Rust append-only JSONL store with the
//! identical logical schema — the same swap the collectors crate already
//! documents ("a SQLite-backed store can implement the exact same shape without
//! touching the aggregation logic"). `window(start,end)` is the `WHERE ts ∈
//! [monday, monday+7d)` projection D2/G3 specify; the on-disk format is an
//! implementation detail behind [`PersistentStore`].

use misaka_mtp_collectors::{
    AttestationRow, ChainFixed, FactStore, GhEvent, Identity, LlmReplicaWork, NodeRecord, Submission, UptimeSample,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A fact row tagged with the wall-clock (or block-time) millisecond at which the
/// underlying activity happened — the value the epoch window filters on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timed<T> {
    /// Activity time in Unix milliseconds (block time for chain facts, event time
    /// for GitHub/forms). This — not ingestion time — decides epoch membership.
    pub ts_ms: u64,
    pub row: T,
}

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("store io error on {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("store row on {path} line {line} is malformed: {reason}")]
    Malformed { path: String, line: usize, reason: String },
}

/// The durable, time-stamped §4.2 fact store. Long-lived rows (`nodes`,
/// `identities`) are upserted; activity rows are appended with a timestamp.
#[derive(Clone, Debug, Default)]
pub struct PersistentStore {
    dir: PathBuf,
    nodes: Vec<NodeRecord>,            // upsert on node_key; long-lived
    identities: Vec<Identity>,         // upsert on id
    uptime_samples: Vec<UptimeSample>, // carries its own at_ms
    attestations: Vec<Timed<AttestationRow>>,
    gh_events: Vec<Timed<GhEvent>>,
    submissions: Vec<Timed<Submission>>,
    chain_fixed: Vec<Timed<ChainFixed>>,
    /// C5 accepted replica slots (ADR-0040 §16″).
    llm_replica_work: Vec<Timed<LlmReplicaWork>>,
}

impl PersistentStore {
    /// An empty in-memory store rooted at `dir` (nothing is read/written until
    /// [`Self::load`] / an `append_*` call).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into(), ..Default::default() }
    }

    fn path(&self, table: &str) -> PathBuf {
        self.dir.join(format!("{table}.jsonl"))
    }

    /// Load every table from `dir`, applying upsert semantics for `nodes` and
    /// `identities` (last line wins per key). Missing files are treated as empty.
    pub fn load(dir: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let mut s = Self::new(dir);
        std::fs::create_dir_all(&s.dir).map_err(|e| StoreError::Io { path: s.dir.display().to_string(), source: e })?;
        for n in read_jsonl::<NodeRecord>(&s.path("nodes"))? {
            s.upsert_node_mem(n);
        }
        for i in read_jsonl::<Identity>(&s.path("identities"))? {
            s.upsert_identity_mem(i);
        }
        s.uptime_samples = read_jsonl(&s.path("uptime_samples"))?;
        s.attestations = read_jsonl(&s.path("attestations"))?;
        s.gh_events = read_jsonl(&s.path("gh_events"))?;
        s.submissions = read_jsonl(&s.path("submissions"))?;
        s.chain_fixed = read_jsonl(&s.path("chain_fixed"))?;
        s.llm_replica_work = read_jsonl(&s.path("llm_replica_work"))?;
        Ok(s)
    }

    fn upsert_node_mem(&mut self, n: NodeRecord) {
        if let Some(e) = self.nodes.iter_mut().find(|x| x.node_key == n.node_key) {
            *e = n;
        } else {
            self.nodes.push(n);
        }
    }

    fn upsert_identity_mem(&mut self, i: Identity) {
        if let Some(e) = self.identities.iter_mut().find(|x| x.id == i.id) {
            e.kind = i.kind;
        } else {
            self.identities.push(i);
        }
    }

    /// Upsert a node and persist the new row (last-write-wins on `node_key`).
    pub fn upsert_node(&mut self, n: NodeRecord) -> Result<(), StoreError> {
        append_line(&self.path("nodes"), &n)?;
        self.upsert_node_mem(n);
        Ok(())
    }

    /// Upsert an identity and persist it.
    pub fn upsert_identity(&mut self, i: Identity) -> Result<(), StoreError> {
        append_line(&self.path("identities"), &i)?;
        self.upsert_identity_mem(i);
        Ok(())
    }

    /// Append an uptime sample (its `at_ms` is the window key).
    pub fn append_sample(&mut self, s: UptimeSample) -> Result<(), StoreError> {
        append_line(&self.path("uptime_samples"), &s)?;
        self.uptime_samples.push(s);
        Ok(())
    }

    /// Append a timestamped attestation fact.
    pub fn append_attestation(&mut self, ts_ms: u64, row: AttestationRow) -> Result<(), StoreError> {
        let t = Timed { ts_ms, row };
        append_line(&self.path("attestations"), &t)?;
        self.attestations.push(t);
        Ok(())
    }

    /// Append a timestamped GitHub bug-report fact (already label-actor-gated, I-MTP-5).
    pub fn append_gh_event(&mut self, ts_ms: u64, row: GhEvent) -> Result<(), StoreError> {
        let t = Timed { ts_ms, row };
        append_line(&self.path("gh_events"), &t)?;
        self.gh_events.push(t);
        Ok(())
    }

    /// Append a timestamped C3/C4 submission (already cap-resolved, I-MTP-6).
    pub fn append_submission(&mut self, ts_ms: u64, row: Submission) -> Result<(), StoreError> {
        let t = Timed { ts_ms, row };
        append_line(&self.path("submissions"), &t)?;
        self.submissions.push(t);
        Ok(())
    }

    /// Append a timestamped C1 fixed chain activity (IBD bench / drill).
    pub fn append_chain_fixed(&mut self, ts_ms: u64, row: ChainFixed) -> Result<(), StoreError> {
        let t = Timed { ts_ms, row };
        append_line(&self.path("chain_fixed"), &t)?;
        self.chain_fixed.push(t);
        Ok(())
    }

    /// Append one accepted, k=2-matched PALW replica slot (C5).
    ///
    /// `ts_ms` must be the job's completion time so the row lands in the epoch the work happened in,
    /// not the epoch it was collected in. The caller is responsible for only appending slots read
    /// off a finality-buried selected chain — a row here is already treated as settled fact.
    pub fn append_llm_replica_work(&mut self, ts_ms: u64, row: LlmReplicaWork) -> Result<(), StoreError> {
        let t = Timed { ts_ms, row };
        append_line(&self.path("llm_replica_work"), &t)?;
        self.llm_replica_work.push(t);
        Ok(())
    }

    /// **The I-MTP-3 projection.** Build a *fresh* single-epoch
    /// [`FactStore`] containing only activity in `[start_ms, end_ms)`. A node is
    /// included iff it has ≥1 in-window uptime sample, so nodes idle this epoch
    /// never emit a spurious zero-point entry, and no prior epoch's facts leak in.
    /// Running this twice on an unchanged store yields byte-identical input.
    pub fn window(&self, start_ms: u64, end_ms: u64) -> FactStore {
        let in_window = |ts: u64| ts >= start_ms && ts < end_ms;

        let samples: Vec<UptimeSample> = self.uptime_samples.iter().filter(|s| in_window(s.at_ms)).cloned().collect();
        let live_keys: BTreeSet<&str> = samples.iter().map(|s| s.node_key.as_str()).collect();
        let nodes: Vec<NodeRecord> = self.nodes.iter().filter(|n| live_keys.contains(n.node_key.as_str())).cloned().collect();

        FactStore {
            // identities do not affect scoring (build_epoch_input ignores them);
            // carry them all for evidence/debugging only.
            identities: self.identities.clone(),
            nodes,
            uptime_samples: samples,
            attestations: self.attestations.iter().filter(|t| in_window(t.ts_ms)).map(|t| t.row.clone()).collect(),
            gh_events: self.gh_events.iter().filter(|t| in_window(t.ts_ms)).map(|t| t.row.clone()).collect(),
            submissions: self.submissions.iter().filter(|t| in_window(t.ts_ms)).map(|t| t.row.clone()).collect(),
            chain_fixed: self.chain_fixed.iter().filter(|t| in_window(t.ts_ms)).map(|t| t.row.clone()).collect(),
            llm_replica_work: self.llm_replica_work.iter().filter(|t| in_window(t.ts_ms)).map(|t| t.row.clone()).collect(),
        }
    }

    /// Total persisted activity-row count (a cheap "how much collected" gauge).
    pub fn len(&self) -> usize {
        self.uptime_samples.len()
            + self.attestations.len()
            + self.gh_events.len()
            + self.submissions.len()
            + self.llm_replica_work.len()
            + self.chain_fixed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn append_line<T: Serialize>(path: &Path, row: &T) -> Result<(), StoreError> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StoreError::Io { path: parent.display().to_string(), source: e })?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| StoreError::Io { path: path.display().to_string(), source: e })?;
    let line = serde_json::to_string(row).expect("fact row JSON is infallible");
    writeln!(f, "{line}").map_err(|e| StoreError::Io { path: path.display().to_string(), source: e })
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, StoreError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreError::Io { path: path.display().to_string(), source: e }),
    };
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row = serde_json::from_str(line).map_err(|e| StoreError::Malformed {
            path: path.display().to_string(),
            line: i + 1,
            reason: e.to_string(),
        })?;
        out.push(row);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_mtp_collectors::{ChainFixedKind, IdentityKind};

    fn node(key: &str, owner: &str, seen: u64) -> NodeRecord {
        NodeRecord {
            node_key: key.into(),
            owner_id: owner.into(),
            ip_v4_24: Some([10, 0, 1]),
            asn: Some(100),
            geo_diverse: false,
            fast_follow: false,
            first_seen_ms: seen,
        }
    }
    fn sample(key: &str, at_ms: u64) -> UptimeSample {
        UptimeSample { node_key: key.into(), at_ms, in_sync: true, vantage: "DE".into(), evidence: format!("s@{at_ms}") }
    }

    #[test]
    fn window_projects_only_in_range_activity() {
        let dir = tempdir();
        let mut s = PersistentStore::new(&dir);
        s.upsert_node(node("n1", "gh:alice", 0)).unwrap();
        // one sample inside [1000,2000), one before, one after.
        s.append_sample(sample("n1", 500)).unwrap();
        s.append_sample(sample("n1", 1500)).unwrap();
        s.append_sample(sample("n1", 2500)).unwrap();
        s.append_submission(
            1200,
            Submission {
                author_id: "gh:alice".into(),
                category: misaka_mtp::Category::Verify,
                base_points: 30,
                evidence: "f1".into(),
            },
        )
        .unwrap();
        s.append_submission(
            2200,
            Submission {
                author_id: "gh:alice".into(),
                category: misaka_mtp::Category::Verify,
                base_points: 99,
                evidence: "f2".into(),
            },
        )
        .unwrap();

        let fs = s.window(1000, 2000);
        assert_eq!(fs.uptime_samples.len(), 1, "only the in-window sample");
        assert_eq!(fs.uptime_samples[0].at_ms, 1500);
        assert_eq!(fs.nodes.len(), 1, "node included because it has an in-window sample");
        assert_eq!(fs.submissions.len(), 1);
        assert_eq!(fs.submissions[0].base_points, 30);
    }

    #[test]
    fn idle_node_is_excluded_from_window() {
        let dir = tempdir();
        let mut s = PersistentStore::new(&dir);
        s.upsert_node(node("n1", "gh:alice", 0)).unwrap();
        s.append_sample(sample("n1", 5)).unwrap(); // outside the window below
        let fs = s.window(1000, 2000);
        assert!(fs.nodes.is_empty(), "no in-window sample → node excluded, no zero-point entry");
        assert!(fs.uptime_samples.is_empty());
    }

    #[test]
    fn persists_and_reloads_with_upsert() {
        let dir = tempdir();
        {
            let mut s = PersistentStore::new(&dir);
            s.upsert_identity(Identity { id: "gh:alice".into(), kind: IdentityKind::Github }).unwrap();
            s.upsert_node(node("n1", "gh:alice", 0)).unwrap();
            // upsert the same node again (owner change) — last write wins on reload.
            s.upsert_node(node("n1", "gh:bob", 9)).unwrap();
            // a sample so the node is live in the window (window() includes a node
            // only when it has an in-window uptime sample — see idle_node test).
            s.append_sample(sample("n1", 1500)).unwrap();
            s.append_chain_fixed(
                1500,
                ChainFixed { author_id: "gh:alice".into(), kind: ChainFixedKind::Drill, evidence: "d1".into() },
            )
            .unwrap();
        }
        let s2 = PersistentStore::load(&dir).unwrap();
        let fs = s2.window(0, u64::MAX);
        assert_eq!(fs.nodes.len(), 1, "node deduped by key on reload");
        assert_eq!(fs.nodes[0].owner_id, "gh:bob", "last write wins");
        assert_eq!(fs.chain_fixed.len(), 1);
        assert_eq!(fs.identities.len(), 1);
    }

    // A unique temp dir without pulling the `tempfile` dev-dep (kept local so the
    // service crate's test deps stay minimal). Unique per call via an atomic.
    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let uniq = format!("mtp-store-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed));
        let p = std::env::temp_dir().join(uniq);
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
