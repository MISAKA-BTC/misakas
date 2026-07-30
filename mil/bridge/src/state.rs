//! Bridge state = an append-only, hash-chained event journal + the in-memory job/provider maps
//! rebuilt from it (the MTP service's append-only-ledger house pattern, minus signing).
//!
//! Every mutation is one [`BridgeEvent`] appended as a JSONL line carrying
//! `root = Hash64_k(journal-domain, prev_root ‖ canonical_event_json)`. Boot replays the file,
//! re-derives every root, and refuses a chain that does not verify — so a restarted bridge
//! resumes exactly where it stopped, and history cannot be silently edited. A torn FINAL line
//! (crash mid-write, no trailing newline) is dropped and truncated away; a torn or altered line
//! ANYWHERE else is a hard error, not a heuristic repair.
//!
//! The head root is the bridge's audit digest: the exact bytes a future consensus seam anchors
//! on-chain (DA-carrier payload), exposed via `/palw/v1/status`.
//!
//! State machine per job (no other transitions exist):
//!
//! ```text
//! submit → Unassigned → Assigned → Matched → Certified      (verdict delivered exactly once
//!              ↑            │          via CertifiedDelivered on first observation)
//!              │            ├→ Mismatch                     (k=2 key disagreement — terminal;
//!              │            │                                an UNRESOLVED DISPUTE, see lib.rs)
//!              └────────────┴ decline / deadline lapse      (requeue)
//! ```
//!
//! Independence rule: an assignment is NEVER offered to the job's own submitter. No flag.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};

use kaspa_hashes::{Hash64, ZERO_HASH64, blake2b_512_keyed};
use serde::{Deserialize, Serialize};

use crate::match_key::{hash64_hex, k2_match};
use crate::wire::{JobSubmissionV1, JobVerdictV1, ReplicaAssignmentV1, ReplicaResultV1};

/// Keyed-BLAKE2b domain for journal chain roots (follows the `misaka-mil-v1/...` convention;
/// bridge-local so a journal root can never be replayed as any consensus value).
pub const BRIDGE_JOURNAL_DOMAIN: &[u8] = b"misaka-palw-bridge-v1/journal";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BridgeEvent {
    JobSubmitted { submission: JobSubmissionV1 },
    Assigned { job_id: String, provider_id: String, deadline_unix_ms: i64 },
    AssignmentLapsed { job_id: String, provider_id: String },
    Declined { job_id: String, provider_id: String, reason: String },
    ResultRecorded { result: ReplicaResultV1, matched: bool },
    /// The `replica_matched` verdict was delivered to the submitter; the job is now certified
    /// (bridge-certified — see lib.rs for what that does NOT mean).
    CertifiedDelivered { job_id: String },
}

#[derive(Serialize, Deserialize)]
struct JournalLine {
    seq: u64,
    unix_ms: i64,
    event: BridgeEvent,
    /// Chain root AFTER applying this event (hex of a 64-byte keyed hash).
    root: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Unassigned,
    Assigned { provider_id: String, deadline_unix_ms: i64 },
    Matched,
    Certified,
    Mismatch,
}

struct JobRecord {
    submission: JobSubmissionV1,
    phase: Phase,
    declines: u32,
    result: Option<ReplicaResultV1>,
}

#[derive(Default, Clone, Serialize)]
pub struct ProviderStats {
    pub jobs_submitted: u64,
    pub replicas_matched: u64,
    pub replicas_mismatched: u64,
    pub declines: u64,
}

pub struct BridgeState {
    path: PathBuf,
    journal: std::fs::File,
    seq: u64,
    head_root: Hash64,
    jobs: HashMap<String, JobRecord>,
    providers: BTreeMap<String, ProviderStats>,
    assignment_deadline_ms: i64,
}

fn chain_root(prev: &Hash64, event_json: &[u8]) -> Hash64 {
    let mut bytes = Vec::with_capacity(64 + event_json.len());
    bytes.extend_from_slice(prev.as_byte_slice());
    bytes.extend_from_slice(event_json);
    blake2b_512_keyed(BRIDGE_JOURNAL_DOMAIN, &bytes)
}

impl BridgeState {
    pub fn open(dir: &Path, assignment_deadline_ms: i64) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let path = dir.join("bridge-journal.jsonl");
        let existing = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("read {}: {e}", path.display())),
        };

        let mut state = Self {
            path: path.clone(),
            journal: std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| format!("open {}: {e}", path.display()))?,
            seq: 0,
            head_root: ZERO_HASH64,
            jobs: HashMap::new(),
            providers: BTreeMap::new(),
            assignment_deadline_ms,
        };

        // Replay + verify. A missing trailing newline marks a torn final append: drop it and
        // truncate the file back to the last good byte so the next append starts clean.
        let mut valid_len = 0usize;
        let mut cursor = 0usize;
        while cursor < existing.len() {
            let Some(nl) = existing[cursor..].iter().position(|&b| b == b'\n') else {
                eprintln!(
                    "[palw-bridge] dropping torn final journal line ({} bytes) — crash recovery",
                    existing.len() - cursor
                );
                break;
            };
            let line = &existing[cursor..cursor + nl];
            let parsed: JournalLine =
                serde_json::from_slice(line).map_err(|e| format!("journal line {}: {e}", state.seq + 1))?;
            if parsed.seq != state.seq + 1 {
                return Err(format!("journal line {}: sequence says {} — chain broken", state.seq + 1, parsed.seq));
            }
            let event_json = serde_json::to_vec(&parsed.event).map_err(|e| e.to_string())?;
            let expect = chain_root(&state.head_root, &event_json);
            if hash64_hex(&expect) != parsed.root {
                return Err(format!(
                    "journal line {}: chain root mismatch — the journal was altered; refusing to load",
                    parsed.seq
                ));
            }
            state.apply(&parsed.event);
            state.seq = parsed.seq;
            state.head_root = expect;
            cursor += nl + 1;
            valid_len = cursor;
        }
        if valid_len < existing.len() {
            // Torn tail: truncate it away before any new append.
            let f = std::fs::OpenOptions::new().write(true).open(&path).map_err(|e| e.to_string())?;
            f.set_len(valid_len as u64).map_err(|e| format!("truncate torn tail: {e}"))?;
            f.sync_all().map_err(|e| e.to_string())?;
            state.journal =
                std::fs::OpenOptions::new().append(true).open(&path).map_err(|e| format!("reopen {}: {e}", path.display()))?;
        }
        Ok(state)
    }

    /// Pure state transition — shared verbatim by live appends and boot replay, so replayed
    /// state can never diverge from lived state.
    fn apply(&mut self, event: &BridgeEvent) {
        match event {
            BridgeEvent::JobSubmitted { submission } => {
                self.providers.entry(submission.provider_id.clone()).or_default().jobs_submitted += 1;
                self.jobs.insert(
                    submission.job_id.clone(),
                    JobRecord { submission: submission.clone(), phase: Phase::Unassigned, declines: 0, result: None },
                );
            }
            BridgeEvent::Assigned { job_id, provider_id, deadline_unix_ms } => {
                if let Some(job) = self.jobs.get_mut(job_id) {
                    job.phase =
                        Phase::Assigned { provider_id: provider_id.clone(), deadline_unix_ms: *deadline_unix_ms };
                }
            }
            BridgeEvent::AssignmentLapsed { job_id, .. } => {
                if let Some(job) = self.jobs.get_mut(job_id) {
                    job.phase = Phase::Unassigned;
                }
            }
            BridgeEvent::Declined { job_id, provider_id, .. } => {
                if let Some(job) = self.jobs.get_mut(job_id) {
                    job.phase = Phase::Unassigned;
                    job.declines += 1;
                }
                self.providers.entry(provider_id.clone()).or_default().declines += 1;
            }
            BridgeEvent::ResultRecorded { result, matched } => {
                if let Some(job) = self.jobs.get_mut(&result.job_id) {
                    job.result = Some(result.clone());
                    job.phase = if *matched { Phase::Matched } else { Phase::Mismatch };
                }
                let stats = self.providers.entry(result.provider_id.clone()).or_default();
                if *matched {
                    stats.replicas_matched += 1;
                } else {
                    stats.replicas_mismatched += 1;
                }
            }
            BridgeEvent::CertifiedDelivered { job_id } => {
                if let Some(job) = self.jobs.get_mut(job_id) {
                    job.phase = Phase::Certified;
                }
            }
        }
    }

    fn append(&mut self, event: BridgeEvent, now_unix_ms: i64) -> Result<(), String> {
        let event_json = serde_json::to_vec(&event).map_err(|e| e.to_string())?;
        let root = chain_root(&self.head_root, &event_json);
        let line = JournalLine { seq: self.seq + 1, unix_ms: now_unix_ms, event, root: hash64_hex(&root) };
        let mut bytes = serde_json::to_vec(&line).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.journal.write_all(&bytes).map_err(|e| format!("append {}: {e}", self.path.display()))?;
        self.journal.sync_data().map_err(|e| format!("fsync {}: {e}", self.path.display()))?;
        self.seq += 1;
        self.head_root = root;
        self.apply(&line.event);
        Ok(())
    }

    // ---- protocol operations ------------------------------------------------------------

    /// Idempotent by job id — an identical re-submission is Ok; a DIFFERENT submission under a
    /// known id is refused (an id is a commitment, not a slot).
    pub fn submit_job(&mut self, submission: &JobSubmissionV1, now_unix_ms: i64) -> Result<(), String> {
        let Some(roots) = &submission.runtime_roots else {
            return Err(
                "runtime_roots required: this bridge coordinates the qi35-serve class, whose match key covers \
                 the engine execution roots (update the gateway if it predates ROOTS capture)"
                    .into(),
            );
        };
        // Validate every hex field now — a malformed submission must fail ITS OWN request, not
        // the eventual replica's.
        crate::match_key::build_match_key(
            &submission.job_id,
            submission.max_new,
            &submission.prompt_ids,
            &submission.output_root,
            roots,
        )?;
        if let Some(existing) = self.jobs.get(&submission.job_id) {
            if existing.submission == *submission {
                return Ok(());
            }
            return Err(format!("job {} already exists with different content", submission.job_id));
        }
        self.append(BridgeEvent::JobSubmitted { submission: submission.clone() }, now_unix_ms)
    }

    pub fn fetch_verdicts(
        &mut self,
        job_ids: &[String],
        now_unix_ms: i64,
    ) -> Result<Vec<(String, JobVerdictV1)>, String> {
        let mut out = Vec::new();
        for id in job_ids {
            let Some(job) = self.jobs.get(id) else { continue };
            match &job.phase {
                Phase::Matched => {
                    out.push((id.clone(), JobVerdictV1::ReplicaMatched));
                    // First observation promotes: the submitter sees the intermediate state
                    // exactly once, and the promotion itself is journaled.
                    self.append(BridgeEvent::CertifiedDelivered { job_id: id.clone() }, now_unix_ms)?;
                }
                Phase::Certified => out.push((id.clone(), JobVerdictV1::Certified)),
                Phase::Mismatch => out.push((id.clone(), JobVerdictV1::Mismatch)),
                Phase::Unassigned | Phase::Assigned { .. } => {}
            }
        }
        Ok(out)
    }

    pub fn fetch_assignments(
        &mut self,
        provider_id: &str,
        now_unix_ms: i64,
    ) -> Result<Vec<ReplicaAssignmentV1>, String> {
        // Lapse pass: assigned-but-silent replicas lose the claim (journaled, then requeued).
        let lapsed: Vec<(String, String)> = self
            .jobs
            .iter()
            .filter_map(|(id, job)| match &job.phase {
                Phase::Assigned { provider_id, deadline_unix_ms } if now_unix_ms > *deadline_unix_ms => {
                    Some((id.clone(), provider_id.clone()))
                }
                _ => None,
            })
            .collect();
        for (job_id, holder) in lapsed {
            self.append(BridgeEvent::AssignmentLapsed { job_id, provider_id: holder }, now_unix_ms)?;
        }

        // Offer pass. THE independence rule lives here: submitter ≠ replica, always.
        let offer: Vec<String> = self
            .jobs
            .iter()
            .filter(|(_, job)| job.phase == Phase::Unassigned && job.submission.provider_id != provider_id)
            .map(|(id, _)| id.clone())
            .collect();
        let mut out = Vec::new();
        for job_id in offer {
            let deadline = now_unix_ms + self.assignment_deadline_ms;
            self.append(
                BridgeEvent::Assigned { job_id: job_id.clone(), provider_id: provider_id.to_string(), deadline_unix_ms: deadline },
                now_unix_ms,
            )?;
            let job = &self.jobs[&job_id];
            out.push(ReplicaAssignmentV1 {
                job_id,
                prompt_ids: job.submission.prompt_ids.clone(),
                max_new: job.submission.max_new,
                deadline_unix_ms: deadline,
            });
        }
        Ok(out)
    }

    pub fn decline_assignment(
        &mut self,
        job_id: &str,
        provider_id: &str,
        reason: &str,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        let Some(job) = self.jobs.get(job_id) else { return Err(format!("unknown job {job_id}")) };
        match &job.phase {
            Phase::Assigned { provider_id: holder, .. } if holder == provider_id => self.append(
                BridgeEvent::Declined {
                    job_id: job_id.to_string(),
                    provider_id: provider_id.to_string(),
                    reason: reason.to_string(),
                },
                now_unix_ms,
            ),
            _ => Ok(()), // not the holder (or already moved on): a stale decline is a no-op
        }
    }

    /// Record B's result and decide the match with the node's k=2 rule. Returns whether it
    /// matched. A late result forfeits the claim (job requeued) instead of deciding anything.
    pub fn submit_replica_result(&mut self, result: &ReplicaResultV1, now_unix_ms: i64) -> Result<bool, String> {
        let Some(job) = self.jobs.get(&result.job_id) else {
            return Err(format!("unknown job {}", result.job_id));
        };
        match &job.phase {
            Phase::Assigned { provider_id, deadline_unix_ms } if provider_id == &result.provider_id => {
                if now_unix_ms > *deadline_unix_ms {
                    let holder = provider_id.clone();
                    self.append(
                        BridgeEvent::AssignmentLapsed { job_id: result.job_id.clone(), provider_id: holder },
                        now_unix_ms,
                    )?;
                    return Err(format!("job {} deadline passed — result ignored, job requeued", result.job_id));
                }
            }
            _ => return Err(format!("job {} is not assigned to {}", result.job_id, result.provider_id)),
        }
        let Some(replica_roots) = &result.runtime_roots else {
            // A replica that cannot produce roots failed the CLASS, not the match: refuse the
            // result and leave the claim to lapse — never brand the JOB (and thereby the
            // submitter's turn) with a mismatch it did not earn.
            return Err("runtime_roots required on replica results (qi35-serve class)".into());
        };
        let submission = &self.jobs[&result.job_id].submission;
        let submitter_roots = submission.runtime_roots.as_ref().expect("enforced at submit");
        let matched = k2_match(
            &submission.job_id,
            submission.max_new,
            &submission.prompt_ids,
            &submission.output_root,
            submitter_roots,
            &result.output_root,
            replica_roots,
        )?
        .is_some();
        self.append(BridgeEvent::ResultRecorded { result: result.clone(), matched }, now_unix_ms)?;
        Ok(matched)
    }

    // ---- introspection ------------------------------------------------------------------

    pub fn head_root_hex(&self) -> String {
        hash64_hex(&self.head_root)
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn status_json(&self) -> serde_json::Value {
        let mut phases: BTreeMap<&'static str, u64> = BTreeMap::new();
        for job in self.jobs.values() {
            let key = match job.phase {
                Phase::Unassigned => "unassigned",
                Phase::Assigned { .. } => "assigned",
                Phase::Matched => "matched",
                Phase::Certified => "certified",
                Phase::Mismatch => "mismatch",
            };
            *phases.entry(key).or_default() += 1;
        }
        serde_json::json!({
            "journal_seq": self.seq,
            "journal_head_root": self.head_root_hex(),
            "jobs": phases,
            "providers": self.providers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::RuntimeRootsV1;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("palw-bridge-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn roots() -> RuntimeRootsV1 {
        RuntimeRootsV1 { route: "aa11".into(), kv: "bb22".into(), state: "cc33".into() }
    }

    fn submission(job: &str, provider: &str) -> JobSubmissionV1 {
        JobSubmissionV1 {
            job_id: job.into(),
            provider_id: provider.into(),
            prompt_ids: vec![1, 2, 3],
            max_new: 64,
            output_root: "dd44".into(),
            receipt_json: None,
            runtime_roots: Some(roots()),
        }
    }

    fn result(job: &str, provider: &str, output_root: &str, r: Option<RuntimeRootsV1>) -> ReplicaResultV1 {
        ReplicaResultV1 { job_id: job.into(), provider_id: provider.into(), output_root: output_root.into(), runtime_roots: r }
    }

    #[test]
    fn full_pipeline_and_independence() {
        let mut s = BridgeState::open(&dir("pipeline"), 120_000).unwrap();
        s.submit_job(&submission("j1", "prov-a"), 1_000).unwrap();
        s.submit_job(&submission("j1", "prov-a"), 1_001).unwrap(); // idempotent
        assert!(s.submit_job(&submission("j1", "prov-b"), 1_002).is_err(), "id reuse with different content");

        // Independence: the submitter is never offered its own job — no flag exists to allow it.
        assert!(s.fetch_assignments("prov-a", 2_000).unwrap().is_empty());

        let got = s.fetch_assignments("prov-b", 2_000).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].prompt_ids, vec![1, 2, 3]);
        assert!(s.fetch_assignments("prov-b", 2_001).unwrap().is_empty(), "claimed once");
        // A third provider can't answer for prov-b's claim.
        assert!(s.submit_replica_result(&result("j1", "prov-c", "dd44", Some(roots())), 2_500).is_err());

        assert!(s.submit_replica_result(&result("j1", "prov-b", "dd44", Some(roots())), 3_000).unwrap());
        // Verdict progression: matched exactly once, then certified.
        let v = s.fetch_verdicts(&["j1".into()], 3_500).unwrap();
        assert_eq!(v, vec![("j1".to_string(), JobVerdictV1::ReplicaMatched)]);
        let v = s.fetch_verdicts(&["j1".into()], 3_600).unwrap();
        assert_eq!(v, vec![("j1".to_string(), JobVerdictV1::Certified)]);
    }

    #[test]
    fn k2_mismatch_on_divergent_execution() {
        let mut s = BridgeState::open(&dir("mismatch"), 120_000).unwrap();
        s.submit_job(&submission("j1", "prov-a"), 1).unwrap();
        let _ = s.fetch_assignments("prov-b", 2).unwrap();
        // Same output root, different ROUTING root: output-only matching would pass this;
        // the real key refuses it.
        let divergent = RuntimeRootsV1 { route: "ffff".into(), kv: "bb22".into(), state: "cc33".into() };
        assert!(!s.submit_replica_result(&result("j1", "prov-b", "dd44", Some(divergent)), 3).unwrap());
        let v = s.fetch_verdicts(&["j1".into()], 4).unwrap();
        assert_eq!(v, vec![("j1".to_string(), JobVerdictV1::Mismatch)]);
    }

    #[test]
    fn class_requires_roots_on_both_sides() {
        let mut s = BridgeState::open(&dir("classreq"), 120_000).unwrap();
        let mut no_roots = submission("j1", "prov-a");
        no_roots.runtime_roots = None;
        assert!(s.submit_job(&no_roots, 1).is_err(), "submission without roots refused");

        s.submit_job(&submission("j2", "prov-a"), 2).unwrap();
        let _ = s.fetch_assignments("prov-b", 3).unwrap();
        // Result without roots: refused (class failure), NOT a mismatch — the job requeues via
        // deadline lapse instead of branding the submitter's turn.
        assert!(s.submit_replica_result(&result("j2", "prov-b", "dd44", None), 4).is_err());
        assert!(s.fetch_verdicts(&["j2".into()], 5).unwrap().is_empty(), "no verdict was reached");
    }

    #[test]
    fn decline_and_deadline_requeue() {
        let mut s = BridgeState::open(&dir("requeue"), 1_000).unwrap();
        s.submit_job(&submission("j1", "prov-a"), 1_000).unwrap();
        let got = s.fetch_assignments("prov-b", 1_000).unwrap();
        assert_eq!(got.len(), 1);
        s.decline_assignment("j1", "prov-b", "over capacity", 1_100).unwrap();
        // Requeued: another provider claims it.
        let got = s.fetch_assignments("prov-c", 1_200).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].deadline_unix_ms, 2_200);
        // Silent past the deadline ⇒ lapse on the next poll; then a late result is refused.
        let got = s.fetch_assignments("prov-d", 5_000).unwrap();
        assert_eq!(got.len(), 1, "lapsed claim re-offered");
        assert!(s.submit_replica_result(&result("j1", "prov-c", "dd44", Some(roots())), 5_100).is_err());
    }

    #[test]
    fn journal_replay_restores_state_and_detects_tamper() {
        let d = dir("durability");
        let head = {
            let mut s = BridgeState::open(&d, 120_000).unwrap();
            s.submit_job(&submission("j1", "prov-a"), 1).unwrap();
            let _ = s.fetch_assignments("prov-b", 2).unwrap();
            assert!(s.submit_replica_result(&result("j1", "prov-b", "dd44", Some(roots())), 3).unwrap());
            s.head_root_hex()
        };
        // Reopen: same head, same state — the matched job still delivers its verdicts.
        let mut s = BridgeState::open(&d, 120_000).unwrap();
        assert_eq!(s.head_root_hex(), head);
        let v = s.fetch_verdicts(&["j1".into()], 10).unwrap();
        assert_eq!(v, vec![("j1".to_string(), JobVerdictV1::ReplicaMatched)]);

        // Torn final line (crash mid-append): dropped + truncated, the rest loads.
        let path = d.join("bridge-journal.jsonl");
        let mut bytes = std::fs::read(&path).unwrap();
        let intact = bytes.len();
        bytes.extend_from_slice(b"{\"seq\":999,\"torn");
        std::fs::write(&path, &bytes).unwrap();
        let s2 = BridgeState::open(&d, 120_000).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), intact as u64, "torn tail truncated away");
        drop(s2);

        // Tamper INSIDE the chain: flip one byte of the first line ⇒ refuse to load.
        let mut bytes = std::fs::read(&path).unwrap();
        let flip = bytes.iter().position(|&b| b == b'1').unwrap();
        bytes[flip] = b'2';
        std::fs::write(&path, &bytes).unwrap();
        assert!(BridgeState::open(&d, 120_000).is_err(), "altered journal must refuse to load");
    }
}
