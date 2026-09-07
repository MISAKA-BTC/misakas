//! Append-only signed-ledger archive (ADR-0038 D6, I-MTP-10, I-MTP-13).
//!
//! The core [`EpochLedger`] has no `supersedes` field — correctly, since the
//! signed digest must stay minimal. Corrections are a **service-layer envelope**:
//! a reissued epoch is a new *fully signed* file `epoch-<n>.<issue>.jsonl` (issue
//! starts at 0), plus an unsigned entry in `index.json` recording
//! `{epoch, issue, supersedes, reason, appeal_url}`. Old issues are **never
//! deleted**; the query surface (D3) is a verbatim mirror of these files.
//!
//! **Finality horizon (I-MTP-13).** A published epoch becomes immutable once the
//! next monthly cumulative snapshot that includes it is published — after that a
//! late supersede cannot quietly rewrite old history. [`LedgerArchive::publish`]
//! refuses any write at or below the frozen epoch; corrections past the horizon
//! must be *forward* adjustment facts in a current epoch.

use misaka_mtp::EpochLedger;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum ArchiveError {
    #[error("archive io error on {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("index.json is malformed: {0}")]
    Index(String),
    #[error("a signed JSONL is malformed: {0}")]
    Ledger(String),
    #[error("refusing to publish an unsigned ledger for epoch {0}")]
    Unsigned(u64),
    #[error("epoch {epoch} is at/under the finality horizon (frozen through {frozen_through}); correct forward, not in place")]
    PastFinalityHorizon { epoch: u64, frozen_through: u64 },
    #[error("issue file {file} already exists — the archive is append-only")]
    IssueExists { file: String },
    #[error("no published issue for epoch {0}")]
    NoSuchEpoch(u64),
    #[error("no issue {issue} for epoch {epoch}")]
    NoSuchIssue { epoch: u64, issue: u32 },
}

/// One line of `index.json` — the unsigned supersede envelope around a signed
/// ledger file. Everything trust-critical is in the signed file itself; this
/// envelope only records the *ordering* of corrections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub epoch: u64,
    /// Reissue counter; 0 is the first publication of the epoch.
    pub issue: u32,
    /// The issue this one supersedes (`issue - 1`), or `None` for issue 0.
    pub supersedes: Option<u32>,
    /// hex of the signed ledger's `digest()` — binds this envelope to the exact file.
    pub digest: String,
    /// The signed-file name, relative to the archive dir.
    pub file: String,
    /// The published `EpochInput` (facts) sidecar file name, if facts were
    /// published alongside the ledger (enables the D3 trustless recompute). `None`
    /// for a ledger-only publish.
    #[serde(default)]
    pub input_file: Option<String>,
    /// Correction reason (empty for issue 0).
    #[serde(default)]
    pub reason: String,
    /// Appeal issue URL that motivated a reissue (empty for issue 0).
    #[serde(default)]
    pub appeal_url: String,
    /// Whether the epoch's appeal window has closed with no open appeal.
    #[serde(default)]
    pub finalized: bool,
}

/// The on-disk `index.json` document: the ordered envelope list plus the finality
/// horizon. Serialized pretty so a human reviewer can read the correction history.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Index {
    entries: Vec<IndexEntry>,
    /// Epochs `<= frozen_through_epoch` are immutable (I-MTP-13); a monthly
    /// snapshot advances it. `None` before the first snapshot.
    #[serde(default)]
    frozen_through_epoch: Option<u64>,
}

/// The append-only, signed-ledger archive rooted at a `points/` directory.
pub struct LedgerArchive {
    dir: PathBuf,
    index: Index,
}

impl LedgerArchive {
    /// Open (or create) the archive at `dir`, loading `index.json` if present.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, ArchiveError> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).map_err(|e| ArchiveError::Io { path: dir.display().to_string(), source: e })?;
        let index_path = dir.join("index.json");
        let index = match std::fs::read_to_string(&index_path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| ArchiveError::Index(e.to_string()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Index::default(),
            Err(e) => return Err(ArchiveError::Io { path: index_path.display().to_string(), source: e }),
        };
        Ok(Self { dir, index })
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join("index.json")
    }

    fn ledger_file_name(epoch: u64, issue: u32) -> String {
        format!("epoch-{epoch}.{issue}.jsonl")
    }

    fn input_file_name(epoch: u64, issue: u32) -> String {
        format!("epoch-{epoch}.{issue}.input.json")
    }

    /// The next issue number to publish for `epoch` (0 if none exists yet).
    pub fn next_issue(&self, epoch: u64) -> u32 {
        self.index.entries.iter().filter(|e| e.epoch == epoch).map(|e| e.issue + 1).max().unwrap_or(0)
    }

    /// The current finality horizon (epochs `<=` this are immutable), if a monthly
    /// snapshot has frozen anything yet.
    pub fn frozen_through(&self) -> Option<u64> {
        self.index.frozen_through_epoch
    }

    /// Publish a signed ledger as a new issue, ledger only (D6). See
    /// [`Self::publish_with_input`] to also publish the facts sidecar (enables the
    /// D3 trustless recompute).
    pub fn publish(&mut self, ledger: &EpochLedger, reason: &str, appeal_url: &str) -> Result<IndexEntry, ArchiveError> {
        self.publish_inner(ledger, None, reason, appeal_url)
    }

    /// Publish a signed ledger **and** its `EpochInput` facts JSON as a new issue.
    /// The facts sidecar is what makes a verifier's recompute self-contained: it
    /// can feed `input_json` through `score_epoch` and byte-compare the ledger,
    /// without trusting the operator (ADR-0038 D3 recipe step 3).
    pub fn publish_with_input(
        &mut self,
        ledger: &EpochLedger,
        input_json: &str,
        reason: &str,
        appeal_url: &str,
    ) -> Result<IndexEntry, ArchiveError> {
        self.publish_inner(ledger, Some(input_json), reason, appeal_url)
    }

    /// Refuses an unsigned ledger, refuses any publish of an epoch at/under the
    /// finality horizon (I-MTP-13), and refuses to overwrite an existing issue file
    /// (append-only). Files are written first, then the index entry is appended and
    /// `index.json` rewritten, so a crash between the two leaves a readable-but-
    /// unindexed file rather than a dangling index row.
    fn publish_inner(
        &mut self,
        ledger: &EpochLedger,
        input_json: Option<&str>,
        reason: &str,
        appeal_url: &str,
    ) -> Result<IndexEntry, ArchiveError> {
        if ledger.sig_mldsa87.is_none() {
            return Err(ArchiveError::Unsigned(ledger.epoch));
        }
        if let Some(frozen) = self.index.frozen_through_epoch
            && ledger.epoch <= frozen
        {
            return Err(ArchiveError::PastFinalityHorizon { epoch: ledger.epoch, frozen_through: frozen });
        }
        let issue = self.next_issue(ledger.epoch);
        // `.then`, not `.then_some`: the latter evaluates `issue - 1` eagerly and
        // underflows when issue == 0.
        let supersedes = (issue > 0).then(|| issue - 1);
        let file = Self::ledger_file_name(ledger.epoch, issue);
        let path = self.dir.join(&file);
        if path.exists() {
            return Err(ArchiveError::IssueExists { file });
        }
        let digest = faster_hex::hex_string(&ledger.digest().as_bytes());
        // Write the signed ledger (a single JSONL line + newline).
        let jsonl = format!("{}\n", ledger.to_jsonl());
        std::fs::write(&path, jsonl).map_err(|e| ArchiveError::Io { path: path.display().to_string(), source: e })?;

        // Write the facts sidecar, if provided.
        let input_file = match input_json {
            Some(json) => {
                let name = Self::input_file_name(ledger.epoch, issue);
                let ipath = self.dir.join(&name);
                std::fs::write(&ipath, json).map_err(|e| ArchiveError::Io { path: ipath.display().to_string(), source: e })?;
                Some(name)
            }
            None => None,
        };

        let entry = IndexEntry {
            epoch: ledger.epoch,
            issue,
            supersedes,
            digest,
            file,
            input_file,
            reason: reason.to_string(),
            appeal_url: appeal_url.to_string(),
            finalized: false,
        };
        self.index.entries.push(entry.clone());
        self.write_index()?;
        Ok(entry)
    }

    /// Mark an epoch's latest issue final (its appeal window closed with no open
    /// appeal). Idempotent; a no-op message value, so `finalize` twice is safe.
    pub fn finalize(&mut self, epoch: u64) -> Result<(), ArchiveError> {
        let latest_issue = self.latest(epoch).map(|e| e.issue).ok_or(ArchiveError::NoSuchEpoch(epoch))?;
        for e in self.index.entries.iter_mut() {
            if e.epoch == epoch && e.issue == latest_issue {
                e.finalized = true;
            }
        }
        self.write_index()
    }

    /// Advance the finality horizon to `through_epoch` (a monthly cumulative
    /// snapshot, I-MTP-13). Monotonic: a lower value is ignored, so a snapshot can
    /// never *un*-freeze already-immutable history.
    pub fn freeze_through(&mut self, through_epoch: u64) -> Result<(), ArchiveError> {
        let new = Some(self.index.frozen_through_epoch.map_or(through_epoch, |cur| cur.max(through_epoch)));
        if new != self.index.frozen_through_epoch {
            self.index.frozen_through_epoch = new;
            self.write_index()?;
        }
        Ok(())
    }

    /// Every index entry (verbatim), in publication order.
    pub fn entries(&self) -> &[IndexEntry] {
        &self.index.entries
    }

    /// Distinct epochs that have at least one published issue, ascending.
    pub fn epochs(&self) -> Vec<u64> {
        let mut es: Vec<u64> = self.index.entries.iter().map(|e| e.epoch).collect();
        es.sort_unstable();
        es.dedup();
        es
    }

    /// The latest (highest-issue) index entry for `epoch`, or `None`.
    pub fn latest(&self, epoch: u64) -> Option<&IndexEntry> {
        self.index.entries.iter().filter(|e| e.epoch == epoch).max_by_key(|e| e.issue)
    }

    /// All index entries for `epoch`, latest issue first (D3 `GET /epoch/<n>`).
    pub fn issues(&self, epoch: u64) -> Vec<&IndexEntry> {
        let mut v: Vec<&IndexEntry> = self.index.entries.iter().filter(|e| e.epoch == epoch).collect();
        v.sort_by(|a, b| b.issue.cmp(&a.issue));
        v
    }

    /// Read a specific issue's signed ledger back, parsed (structure recovered from
    /// the byte-exact file). Callers verify the signature against the operator key.
    pub fn read_ledger(&self, epoch: u64, issue: u32) -> Result<EpochLedger, ArchiveError> {
        let jsonl = self.read_jsonl(epoch, issue)?;
        serde_json::from_str(jsonl.trim_end()).map_err(|e| ArchiveError::Ledger(e.to_string()))
    }

    /// Read a specific issue's signed JSONL bytes verbatim (the D3 mirror source).
    pub fn read_jsonl(&self, epoch: u64, issue: u32) -> Result<String, ArchiveError> {
        let entry = self
            .index
            .entries
            .iter()
            .find(|e| e.epoch == epoch && e.issue == issue)
            .ok_or(ArchiveError::NoSuchIssue { epoch, issue })?;
        let path = self.dir.join(&entry.file);
        std::fs::read_to_string(&path).map_err(|e| ArchiveError::Io { path: path.display().to_string(), source: e })
    }

    /// The latest issue's signed ledger for `epoch`, parsed.
    pub fn read_latest(&self, epoch: u64) -> Result<EpochLedger, ArchiveError> {
        let issue = self.latest(epoch).map(|e| e.issue).ok_or(ArchiveError::NoSuchEpoch(epoch))?;
        self.read_ledger(epoch, issue)
    }

    /// The published `EpochInput` facts JSON for a specific issue, verbatim, or
    /// `None` if that issue was published ledger-only (no facts sidecar).
    pub fn read_input_json(&self, epoch: u64, issue: u32) -> Result<Option<String>, ArchiveError> {
        let entry = self
            .index
            .entries
            .iter()
            .find(|e| e.epoch == epoch && e.issue == issue)
            .ok_or(ArchiveError::NoSuchIssue { epoch, issue })?;
        match &entry.input_file {
            Some(name) => {
                let path = self.dir.join(name);
                std::fs::read_to_string(&path).map(Some).map_err(|e| ArchiveError::Io { path: path.display().to_string(), source: e })
            }
            None => Ok(None),
        }
    }

    fn write_index(&self) -> Result<(), ArchiveError> {
        let path = self.index_path();
        let json = serde_json::to_string_pretty(&self.index).expect("index JSON is infallible");
        // Write to a temp sibling then rename, so a concurrent reader never sees a
        // half-written index.json.
        let tmp = self.dir.join("index.json.tmp");
        std::fs::write(&tmp, json).map_err(|e| ArchiveError::Io { path: tmp.display().to_string(), source: e })?;
        std::fs::rename(&tmp, &path).map_err(|e| ArchiveError::Io { path: path.display().to_string(), source: e })
    }
}

/// Convenience: the archive directory holds one JSONL per issue plus `index.json`.
/// (Free function so tools that only need the name don't construct an archive.)
pub fn issue_file_name(epoch: u64, issue: u32) -> String {
    LedgerArchive::ledger_file_name(epoch, issue)
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
        let uniq = format!("mtp-archive-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed));
        let p = std::env::temp_dir().join(uniq);
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn signed_ledger(epoch: u64, c1: u64, key: &ValidatorKey) -> EpochLedger {
        let mut l = EpochLedger {
            epoch,
            range: ["2026-09-21T00:00:00Z".into(), "2026-09-28T00:00:00Z".into()],
            network: "testnet-10".into(),
            rules_hash: "aa".into(),
            inputs_hash: "bb".into(),
            scores: vec![ScoreRow { id: "gh:alice".into(), c1, c2: 0, c3: 0, c4: 0, c5: 0, evidence: vec!["ev1".into()] }],
            sig_mldsa87: None,
        };
        l.sign(key);
        l
    }

    #[test]
    fn publish_is_append_only_and_readback_is_byte_exact() {
        let dir = tempdir();
        let key = ValidatorKey::from_seed([1; 32]);
        let mut a = LedgerArchive::open(&dir).unwrap();

        let l = signed_ledger(7, 100_000, &key);
        let entry = a.publish(&l, "", "").unwrap();
        assert_eq!(entry.issue, 0);
        assert_eq!(entry.supersedes, None);

        // read back: structurally identical + signature still verifies.
        let back = a.read_ledger(7, 0).unwrap();
        assert_eq!(back, l);
        assert!(back.verify(key.public_key()));

        // reopening from disk sees the same index.
        let a2 = LedgerArchive::open(&dir).unwrap();
        assert_eq!(a2.entries().len(), 1);
        assert_eq!(a2.latest(7).unwrap().digest, entry.digest);
    }

    #[test]
    fn unsigned_ledger_is_refused() {
        let dir = tempdir();
        let mut a = LedgerArchive::open(&dir).unwrap();
        let unsigned = EpochLedger {
            epoch: 1,
            range: ["a".into(), "b".into()],
            network: "testnet-10".into(),
            rules_hash: "aa".into(),
            inputs_hash: "bb".into(),
            scores: vec![],
            sig_mldsa87: None,
        };
        assert!(matches!(a.publish(&unsigned, "", ""), Err(ArchiveError::Unsigned(1))));
    }

    #[test]
    fn supersede_reissues_and_both_issues_survive() {
        let dir = tempdir();
        let key = ValidatorKey::from_seed([2; 32]);
        let mut a = LedgerArchive::open(&dir).unwrap();

        a.publish(&signed_ledger(9, 100_000, &key), "", "").unwrap();
        // a correction: alice's C1 was wrong; reissue.
        let fixed = signed_ledger(9, 150_000, &key);
        let e1 = a.publish(&fixed, "appeal #12: undercounted uptime", "https://gh/appeal/12").unwrap();
        assert_eq!(e1.issue, 1);
        assert_eq!(e1.supersedes, Some(0));

        // both issues verify independently and are byte-distinct.
        assert_eq!(a.read_ledger(9, 0).unwrap().scores[0].c1, 100_000);
        assert_eq!(a.read_ledger(9, 1).unwrap().scores[0].c1, 150_000);
        // latest / issues ordering.
        assert_eq!(a.latest(9).unwrap().issue, 1);
        assert_eq!(a.issues(9).iter().map(|e| e.issue).collect::<Vec<_>>(), vec![1, 0]);
    }

    #[test]
    fn finality_horizon_blocks_late_supersede() {
        let dir = tempdir();
        let key = ValidatorKey::from_seed([3; 32]);
        let mut a = LedgerArchive::open(&dir).unwrap();
        a.publish(&signed_ledger(5, 100_000, &key), "", "").unwrap();
        a.publish(&signed_ledger(6, 100_000, &key), "", "").unwrap();

        // monthly snapshot freezes epochs through 5.
        a.freeze_through(5).unwrap();
        assert_eq!(a.frozen_through(), Some(5));

        // a supersede of the frozen epoch 5 is refused (I-MTP-13).
        let err = a.publish(&signed_ledger(5, 999_000, &key), "late", "").unwrap_err();
        assert!(matches!(err, ArchiveError::PastFinalityHorizon { epoch: 5, frozen_through: 5 }));
        // epoch 6 is still open — a supersede there is allowed.
        assert!(a.publish(&signed_ledger(6, 120_000, &key), "appeal", "").is_ok());
        // a brand-new epoch 7 above the horizon is allowed.
        assert!(a.publish(&signed_ledger(7, 100_000, &key), "", "").is_ok());

        // freeze is monotonic: a lower snapshot value never un-freezes.
        a.freeze_through(3).unwrap();
        assert_eq!(a.frozen_through(), Some(5));
    }

    #[test]
    fn finalize_marks_latest_issue() {
        let dir = tempdir();
        let key = ValidatorKey::from_seed([4; 32]);
        let mut a = LedgerArchive::open(&dir).unwrap();
        a.publish(&signed_ledger(2, 1, &key), "", "").unwrap();
        a.publish(&signed_ledger(2, 2, &key), "fix", "").unwrap();
        a.finalize(2).unwrap();
        assert!(a.latest(2).unwrap().finalized);
        // the superseded issue 0 stays non-final (it is history, not the answer).
        assert!(!a.issues(2).iter().find(|e| e.issue == 0).unwrap().finalized);
    }
}
