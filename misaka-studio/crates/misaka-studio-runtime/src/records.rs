//! The inference log — one [`InferenceRecord`] per completion, appended to a JSONL file.
//!
//! This is the artefact the later stages of `Inference → Deterministic Execution → Inference Hash
//! → Verification → Compute Credit → PALW → MISAKA Network` consume. Nothing in this version
//! publishes it anywhere; it exists so that when something does, the history is already there
//! and in one shape.
//!
//! JSONL because the operations are append and tail, and because a log that a person can read
//! with `tail -f` and `jq` is one they can audit without this app. A database would be faster at
//! a query nobody makes.
//!
//! # Transcripts are opt-in
//!
//! A record commits to the prompt and the completion with hashes. The **text** is written only
//! when `provenance.keep_transcripts` is on, because a provenance log that quietly duplicates
//! every conversation is a second copy of the user's data in a place they never chose. The
//! commitments are enough for verification; the plaintext is only needed to re-run a job, and
//! that is a decision to make deliberately.

use crate::{Error, Result};
use misaka_studio_core::provenance::InferenceRecord;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// A record plus, optionally, the bytes it commits to.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredRecord {
    #[serde(flatten)]
    pub record: InferenceRecord,
    /// The prompt as sent, when transcripts are kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<String>,
    /// The model this ran on, by Studio id — the human-readable half of `h_M`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

/// Append-only record log with an in-memory tail.
pub struct RecordStore {
    path: PathBuf,
    recent: Mutex<VecDeque<StoredRecord>>,
    max_records: usize,
    enabled: bool,
}

/// How many records are held in memory for the API to serve without touching disk.
const RECENT_CAPACITY: usize = 200;

impl RecordStore {
    /// Open (or create) the log and load its tail.
    pub async fn open(path: PathBuf, max_records: usize, enabled: bool) -> Arc<Self> {
        let recent = load_tail(&path, RECENT_CAPACITY).await;
        Arc::new(RecordStore { path, recent: Mutex::new(recent), max_records, enabled })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Append a record. A failure to write is logged, never propagated: losing the provenance
    /// line must not fail the user's completion, which has already been generated and streamed.
    pub async fn append(&self, stored: StoredRecord) {
        if !self.enabled {
            return;
        }
        {
            let mut recent = self.recent.lock().await;
            if recent.len() == RECENT_CAPACITY {
                recent.pop_front();
            }
            recent.push_back(stored.clone());
        }
        if let Err(e) = self.write_line(&stored).await {
            tracing::warn!("could not append an inference record: {e}");
        }
    }

    async fn write_line(&self, stored: &StoredRecord) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| Error::io(parent.display(), e))?;
        }
        let mut line = serde_json::to_string(stored).map_err(|e| Error::io(self.path.display(), std::io::Error::other(e)))?;
        line.push('\n');
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .map_err(|e| Error::io(self.path.display(), e))?;
        file.write_all(line.as_bytes()).await.map_err(|e| Error::io(self.path.display(), e))?;
        Ok(())
    }

    /// The most recent records, newest first.
    pub async fn list(&self, limit: usize) -> Vec<StoredRecord> {
        let recent = self.recent.lock().await;
        recent.iter().rev().take(limit.min(RECENT_CAPACITY)).cloned().collect()
    }

    pub async fn get(&self, id: &str) -> Option<StoredRecord> {
        self.recent.lock().await.iter().rev().find(|r| r.record.id == id).cloned()
    }

    /// Drop the oldest lines when the log has grown past `max_records`.
    ///
    /// Rewrite-and-rename rather than truncate-in-place: a crash mid-trim otherwise leaves a file
    /// whose first line is half a record, and every later read of the log fails on it.
    pub async fn trim(&self) -> Result<()> {
        let Ok(text) = tokio::fs::read_to_string(&self.path).await else { return Ok(()) };
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.len() <= self.max_records {
            return Ok(());
        }
        let keep = lines[lines.len() - self.max_records..].join("\n");
        let temp = self.path.with_extension("jsonl.tmp");
        tokio::fs::write(&temp, format!("{keep}\n")).await.map_err(|e| Error::io(temp.display(), e))?;
        tokio::fs::rename(&temp, &self.path).await.map_err(|e| Error::io(self.path.display(), e))?;
        Ok(())
    }
}

/// Read the last `count` parseable records.
///
/// Unparseable lines are skipped rather than fatal: a log truncated by a full disk should still
/// give up the records that survived it.
async fn load_tail(path: &Path, count: usize) -> VecDeque<StoredRecord> {
    let Ok(text) = tokio::fs::read_to_string(path).await else { return VecDeque::new() };
    let mut out = VecDeque::with_capacity(count);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<StoredRecord>(line) {
            Ok(record) => {
                if out.len() == count {
                    out.pop_front();
                }
                out.push_back(record);
            }
            Err(e) => tracing::debug!("skipping an unreadable record line: {e}"),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_studio_core::provenance::{InferenceInputs, ModelIdentity, RuntimeDescriptor, RuntimeIdentity, SamplingCommitment};

    fn record(id: &str) -> StoredRecord {
        let model = ModelIdentity::derive("abc", 10, "m.gguf", "repo", "rev");
        let runtime = RuntimeIdentity::derive(RuntimeDescriptor {
            backend: "mock".into(),
            engine_commit: "mock".into(),
            engine_patch_sha256: "unpatched".into(),
            engine_build_number: 0,
            build_profile: "mock/v1".into(),
            class_tag: "misaka-studio-mock/v1".into(),
        });
        StoredRecord {
            record: InferenceRecord::new(
                id,
                InferenceInputs {
                    model: Some(&model),
                    runtime: &runtime,
                    params: SamplingCommitment::default(),
                    prompt: b"q",
                    output: b"a",
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    started_at_unix_ms: 0,
                    duration_ms: 10,
                    time_to_first_token_ms: Some(5),
                },
            ),
            prompt: None,
            completion: None,
            model_id: Some("m".into()),
        }
    }

    #[tokio::test]
    async fn records_append_and_survive_a_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("records.jsonl");

        let store = RecordStore::open(path.clone(), 100, true).await;
        store.append(record("one")).await;
        store.append(record("two")).await;
        assert_eq!(store.list(10).await.len(), 2);
        assert_eq!(store.list(10).await[0].record.id, "two", "newest first");

        let reopened = RecordStore::open(path, 100, true).await;
        assert_eq!(reopened.list(10).await.len(), 2);
        assert!(reopened.get("one").await.is_some());
    }

    #[tokio::test]
    async fn recording_can_be_switched_off_entirely() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("records.jsonl");
        let store = RecordStore::open(path.clone(), 100, false).await;
        store.append(record("one")).await;
        assert!(store.list(10).await.is_empty());
        assert!(!path.exists(), "nothing is written when recording is off");
    }

    #[tokio::test]
    async fn trimming_keeps_the_newest_and_leaves_a_readable_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("records.jsonl");
        let store = RecordStore::open(path.clone(), 3, true).await;
        for i in 0..10 {
            store.append(record(&format!("r{i}"))).await;
        }
        store.trim().await.expect("trims");

        let reopened = RecordStore::open(path, 3, true).await;
        let kept: Vec<String> = reopened.list(10).await.into_iter().map(|r| r.record.id).collect();
        assert_eq!(kept, vec!["r9", "r8", "r7"]);
    }

    #[tokio::test]
    async fn a_corrupt_line_does_not_hide_the_good_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("records.jsonl");
        let store = RecordStore::open(path.clone(), 100, true).await;
        store.append(record("good")).await;
        tokio::fs::OpenOptions::new().append(true).open(&path).await.expect("open").write_all(b"{ truncated\n").await.expect("write");

        let reopened = RecordStore::open(path, 100, true).await;
        assert_eq!(reopened.list(10).await.len(), 1);
    }

    /// The privacy default, asserted: a record on disk carries hashes, not the conversation.
    #[tokio::test]
    async fn a_record_without_transcripts_contains_no_prompt_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("records.jsonl");
        let store = RecordStore::open(path.clone(), 100, true).await;
        store.append(record("private")).await;
        let text = tokio::fs::read_to_string(&path).await.expect("read");
        assert!(!text.contains("\"prompt\""), "the prompt field is absent, not null: {text}");
        assert!(text.contains("inference_hash"));
    }
}
