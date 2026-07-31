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

use crate::arbitration::{DisputeRecord, SlashEvidenceV1, adjudicate, dispute_id, is_escalated, select_auditor, verdict_str};
use crate::chain::{ChainFacts, parse_outpoint};
use crate::challenge::{JobLeaseV1, request_commitment, salted_output_commitment};
use crate::da::{DaCommitmentWire, DaObligation, DaObligationStatus, DaResponseWire, register_obligations, timeout_evidence};
use crate::match_key::{hash64_hex, k2_match};
use crate::provider::{ProviderRegistrationV1, ProviderRegistry, RegisteredProvider};
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
    /// Seam 2: a bonded provider registered (already verified against the chain when accepted).
    ProviderRegistered { provider: RegisteredProvider },
    /// Seam 1: a job challenge was leased against a buried beacon, before any generation.
    ChallengeLeased { lease: JobLeaseV1 },
    /// Seam 3: per-provider DA obligations registered for a job's context object.
    DaObligationsRegistered { obligations: Vec<DaObligation> },
    DaChallenged { obligation_id: String, deadline_daa_score: u64 },
    DaSatisfied { obligation_id: String },
    DaTimedOut { obligation_id: String, evidence_json: String },
    /// Seam 4: a k=2 disagreement opened a dispute.
    DisputeOpened { dispute: DisputeRecord },
    DisputeAuditorSelected { dispute_id: String, auditor: String },
    DisputeAdjudicated { evidence: SlashEvidenceV1 },
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

/// A dispute plus the auditor assignment adjudicating it.
struct DisputeState {
    record: DisputeRecord,
    /// Set once an auditor is drawn; cleared when the auditor's reference run resolves it.
    pending_auditor: Option<String>,
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
    /// Seam 2 — bonded providers registered with this bridge.
    registry: ProviderRegistry,
    /// Seam 1 — outstanding challenge leases, by scheduler job id.
    leases: BTreeMap<String, JobLeaseV1>,
    /// Seam 3 — DA obligations by obligation id.
    obligations: BTreeMap<String, DaObligation>,
    /// Seam 4 — open and adjudicated disputes by dispute id.
    disputes: BTreeMap<String, DisputeState>,
    /// Per-credential mismatch history feeding the repeat-offender rule.
    mismatch_counts: BTreeMap<String, u32>,
    network_id: u32,
}

fn chain_root(prev: &Hash64, event_json: &[u8]) -> Hash64 {
    let mut bytes = Vec::with_capacity(64 + event_json.len());
    bytes.extend_from_slice(prev.as_byte_slice());
    bytes.extend_from_slice(event_json);
    blake2b_512_keyed(BRIDGE_JOURNAL_DOMAIN, &bytes)
}

impl BridgeState {
    pub fn open(dir: &Path, assignment_deadline_ms: i64, network_id: u32) -> Result<Self, String> {
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
            registry: ProviderRegistry::new(network_id),
            leases: BTreeMap::new(),
            obligations: BTreeMap::new(),
            disputes: BTreeMap::new(),
            mismatch_counts: BTreeMap::new(),
            network_id,
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
            BridgeEvent::ProviderRegistered { provider } => {
                self.registry.insert_verified(provider.clone());
            }
            BridgeEvent::ChallengeLeased { lease } => {
                self.leases.insert(lease.scheduler_job_id_hex.clone(), lease.clone());
            }
            BridgeEvent::DaObligationsRegistered { obligations } => {
                for obligation in obligations {
                    self.obligations.insert(obligation.obligation_id_hex.clone(), obligation.clone());
                }
            }
            BridgeEvent::DaChallenged { obligation_id, deadline_daa_score } => {
                if let Some(obligation) = self.obligations.get_mut(obligation_id) {
                    obligation.status = DaObligationStatus::Challenged { deadline_daa_score: *deadline_daa_score };
                }
            }
            BridgeEvent::DaSatisfied { obligation_id } => {
                if let Some(obligation) = self.obligations.get_mut(obligation_id) {
                    obligation.status = DaObligationStatus::Satisfied;
                }
            }
            BridgeEvent::DaTimedOut { obligation_id, .. } => {
                if let Some(obligation) = self.obligations.get_mut(obligation_id) {
                    obligation.status = DaObligationStatus::TimedOut;
                }
            }
            BridgeEvent::DisputeOpened { dispute } => {
                for provider in [&dispute.provider_a, &dispute.provider_b] {
                    *self.mismatch_counts.entry(provider.clone()).or_default() += 1;
                }
                self.disputes.insert(
                    dispute.dispute_id_hex.clone(),
                    DisputeState { record: dispute.clone(), pending_auditor: None },
                );
            }
            BridgeEvent::DisputeAuditorSelected { dispute_id, auditor } => {
                if let Some(state) = self.disputes.get_mut(dispute_id) {
                    state.record.auditor = Some(auditor.clone());
                    state.pending_auditor = Some(auditor.clone());
                }
            }
            BridgeEvent::DisputeAdjudicated { evidence } => {
                if let Some(state) = self.disputes.get_mut(&evidence.dispute_id_hex) {
                    state.record.verdict = Some(evidence.verdict.clone());
                    state.record.slash_targets = evidence.slash_targets.clone();
                    state.pending_auditor = None;
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

    // ---- seam 2: bonded providers ---------------------------------------------------------

    /// Verify a registration against the chain and journal it.
    pub fn register_provider(
        &mut self,
        registration: &ProviderRegistrationV1,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<RegisteredProvider, String> {
        let provider = self.registry.verify_registration(registration, chain)?;
        self.append(BridgeEvent::ProviderRegistered { provider: provider.clone() }, now_unix_ms)?;
        Ok(provider)
    }

    /// Authenticate a signed request (ML-DSA-87 session key + live bond re-check).
    pub fn authenticate(
        &self,
        bond_outpoint: &str,
        route: &str,
        body: &[u8],
        signature_hex: &str,
        chain: &dyn ChainFacts,
    ) -> Result<&RegisteredProvider, String> {
        let beacon = chain.beacon()?;
        self.registry.authenticate(bond_outpoint, route, body, signature_hex, chain, beacon.current_epoch)
    }

    pub fn registered_provider(&self, bond_outpoint: &str) -> Option<&RegisteredProvider> {
        self.registry.get(bond_outpoint)
    }

    // ---- seam 1: challenge leases -----------------------------------------------------------

    /// Issue a challenge BEFORE the provider generates. The lease binds the beacon, the exact
    /// prompt, and the requesting credential; a submission that does not reproduce all three is
    /// refused. This is what makes regenerate-until-you-like-it impossible.
    pub fn lease_challenge(
        &mut self,
        requester_bond: &str,
        prompt_token_ids: &[u32],
        max_new: u32,
        class_label: &[u8],
        shape_id: u16,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<JobLeaseV1, String> {
        let provider = self
            .registry
            .get(requester_bond)
            .ok_or_else(|| format!("provider {requester_bond} is not registered with this bridge"))?;
        let credential = provider.credential()?;
        let beacon = chain.beacon()?;
        let commitment = request_commitment(prompt_token_ids, max_new, class_label);
        // The scheduler job id binds the requester and the request, so two providers asking for
        // the same prompt get different challenges and neither can claim the other's lease.
        let scheduler_job_id = blake2b_512_keyed(
            b"misaka-palw-bridge-v1/scheduler-job-id",
            &{
                let mut preimage = Vec::with_capacity(136);
                preimage.extend_from_slice(credential.as_byte_slice());
                preimage.extend_from_slice(commitment.as_byte_slice());
                preimage.extend_from_slice(&beacon.epoch.to_le_bytes());
                preimage
            },
        );
        let lease = JobLeaseV1::issue(self.network_id, &beacon, &scheduler_job_id, &credential, &commitment, shape_id)?;
        lease.verify_self_consistent()?;
        if let Some(existing) = self.leases.get(&lease.scheduler_job_id_hex) {
            // Same inputs ⇒ same challenge; hand the existing lease back rather than minting a
            // second one (idempotent, and it cannot be used to re-roll).
            return Ok(existing.clone());
        }
        self.append(BridgeEvent::ChallengeLeased { lease: lease.clone() }, now_unix_ms)?;
        Ok(lease)
    }

    pub fn lease_for_challenge(&self, job_challenge_hex: &str) -> Option<&JobLeaseV1> {
        self.leases.values().find(|lease| lease.job_challenge_hex == job_challenge_hex)
    }

    /// Enforce the lease against a submission: the challenge must be one we issued, to this
    /// requester, for this exact prompt, unexpired — and the output commitment must be the
    /// salted receipt-v3 commitment under that challenge.
    pub fn check_lease(
        &self,
        submission: &JobSubmissionV1,
        requester_bond: &str,
        class_label: &[u8],
        current_epoch: u64,
    ) -> Result<(), String> {
        let Some(challenge_hex) = &submission.job_challenge else {
            return Err("job_challenge required: lease one from POST /palw/v1/challenges before generating".into());
        };
        let lease = self
            .lease_for_challenge(challenge_hex)
            .ok_or("job_challenge was not issued by this bridge (or has been forgotten)")?;
        let provider = self
            .registry
            .get(requester_bond)
            .ok_or_else(|| format!("provider {requester_bond} is not registered"))?;
        lease.accepts(&submission.prompt_ids, submission.max_new, class_label, &provider.credential()?, current_epoch)?;

        let Some(output_ids) = &submission.output_token_ids else {
            return Err("output_token_ids required for the salted output commitment".into());
        };
        let expected = salted_output_commitment(output_ids, &lease.job_challenge()?);
        let claimed = submission
            .output_commitment
            .as_deref()
            .ok_or("output_commitment required (receipt-v3 salted commitment)")?;
        if hash64_hex(&expected) != claimed {
            return Err("output_commitment is not output_commitment_v3(output_token_ids, job_challenge)".into());
        }
        Ok(())
    }

    // ---- seam 3: DA -------------------------------------------------------------------------

    /// Register the submitter's DA obligations for a job's context object. Chunk indices come
    /// from the beacon-driven sampler, so nobody chooses which chunk they must retain.
    pub fn register_da(
        &mut self,
        job_id: &str,
        provider_bond: &str,
        commitment: &DaCommitmentWire,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<Vec<DaObligation>, String> {
        let beacon = chain.beacon()?;
        let buried = beacon.to_buried()?;
        let obligations = register_obligations(job_id, provider_bond, commitment, &buried, beacon.observed_daa_score)?;
        self.append(BridgeEvent::DaObligationsRegistered { obligations: obligations.clone() }, now_unix_ms)?;
        Ok(obligations)
    }

    /// Open DA challenges for a provider's pending obligations (what the provider polls for).
    pub fn open_da_challenges(
        &mut self,
        provider_bond: &str,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<Vec<DaObligation>, String> {
        let beacon = chain.beacon()?;
        let now_daa = beacon.observed_daa_score;
        let window = crate::da::policy().response_window_daa;
        let pending: Vec<String> = self
            .obligations
            .values()
            .filter(|o| o.provider_bond == provider_bond && o.status == DaObligationStatus::Pending)
            .map(|o| o.obligation_id_hex.clone())
            .collect();
        let mut opened = Vec::new();
        for id in pending {
            // The node clamps the deadline to the retention window; do the same.
            let retention = self.obligations[&id].retention_until_daa_score;
            let deadline = now_daa.saturating_add(window).min(retention);
            self.append(BridgeEvent::DaChallenged { obligation_id: id.clone(), deadline_daa_score: deadline }, now_unix_ms)?;
            opened.push(self.obligations[&id].clone());
        }
        Ok(opened)
    }

    /// Verify a chunk proof and satisfy the obligation.
    pub fn answer_da_challenge(
        &mut self,
        response: &DaResponseWire,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        let obligation = self
            .obligations
            .get(&response.obligation_id_hex)
            .ok_or_else(|| format!("unknown obligation {}", response.obligation_id_hex))?
            .clone();
        if obligation.provider_bond != response.provider_bond {
            return Err("response comes from a different provider than the obligation".into());
        }
        let deadline = match obligation.status {
            DaObligationStatus::Challenged { deadline_daa_score } => deadline_daa_score,
            DaObligationStatus::Pending => return Err("obligation has not been challenged".into()),
            DaObligationStatus::Satisfied => return Ok(()),
            DaObligationStatus::TimedOut => return Err("obligation already timed out".into()),
        };
        let beacon = chain.beacon()?;
        if beacon.observed_daa_score > deadline {
            return Err(format!("response is past the deadline ({deadline})"));
        }
        response.verify(&obligation)?;
        self.append(BridgeEvent::DaSatisfied { obligation_id: obligation.obligation_id_hex }, now_unix_ms)
    }

    /// Sweep challenged-but-unanswered obligations past their deadline into timeout evidence.
    pub fn sweep_da_timeouts(&mut self, chain: &dyn ChainFacts, now_unix_ms: i64) -> Result<Vec<String>, String> {
        let beacon = chain.beacon()?;
        let now_daa = beacon.observed_daa_score;
        let expired: Vec<DaObligation> = self
            .obligations
            .values()
            .filter(|o| matches!(o.status, DaObligationStatus::Challenged { deadline_daa_score } if now_daa > deadline_daa_score))
            .cloned()
            .collect();
        let mut out = Vec::new();
        for obligation in expired {
            let evidence = timeout_evidence(self.network_id, &obligation)?;
            let evidence_json = serde_json::json!({
                "version": evidence.version,
                "network_id": evidence.network_id,
                "challenge_id": hash64_hex(&evidence.challenge_id),
                "provider_bond": crate::chain::format_outpoint(&evidence.provider_bond),
            })
            .to_string();
            self.append(
                BridgeEvent::DaTimedOut { obligation_id: obligation.obligation_id_hex.clone(), evidence_json },
                now_unix_ms,
            )?;
            out.push(obligation.obligation_id_hex);
        }
        Ok(out)
    }

    pub fn da_obligations_for(&self, provider_bond: &str) -> Vec<DaObligation> {
        self.obligations.values().filter(|o| o.provider_bond == provider_bond).cloned().collect()
    }

    // ---- seam 4: arbitration ----------------------------------------------------------------

    /// Open a dispute for a mismatched job, run the real escalation draw, and (if escalated)
    /// draw an auditor from the bonded set excluding both disputants and their operator groups.
    pub fn open_dispute(
        &mut self,
        job_id: &str,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<Option<DisputeRecord>, String> {
        let job = self.jobs.get(job_id).ok_or_else(|| format!("unknown job {job_id}"))?;
        let Some(result) = &job.result else { return Err("job has no replica result to dispute".into()) };
        let submitter = job.submission.provider_id.clone();
        let replica = result.provider_id.clone();
        let (Some(a_roots), Some(b_roots)) = (&job.submission.runtime_roots, &result.runtime_roots) else {
            return Err("both sides must carry runtime roots to open a dispute".into());
        };
        // The k=2 field that differs is the output commitment; carry each side's own.
        let a_key = crate::match_key::build_match_key(
            job_id, job.submission.max_new, &job.submission.prompt_ids, &job.submission.output_root, a_roots,
        )?;
        let b_key = crate::match_key::build_match_key(
            job_id, job.submission.max_new, &job.submission.prompt_ids, &result.output_root, b_roots,
        )?;
        let beacon = chain.beacon()?;
        let record = DisputeRecord {
            dispute_id_hex: hash64_hex(&dispute_id(job_id, &parse_outpoint(&submitter)?, &parse_outpoint(&replica)?)),
            job_id: job_id.to_string(),
            provider_a: submitter.clone(),
            provider_b: replica.clone(),
            output_a_hex: hash64_hex(&a_key.output_commitment),
            output_b_hex: hash64_hex(&b_key.output_commitment),
            beacon_epoch: beacon.epoch,
            escalated: false,
            auditor: None,
            verdict: None,
            slash_targets: Vec::new(),
        };
        if self.disputes.contains_key(&record.dispute_id_hex) {
            return Ok(Some(self.disputes[&record.dispute_id_hex].record.clone()));
        }
        let seed = beacon.seed()?;
        let prior_a = self.mismatch_counts.get(&submitter).copied().unwrap_or(0);
        let prior_b = self.mismatch_counts.get(&replica).copied().unwrap_or(0);
        let escalated = is_escalated(&record, &seed, prior_a, prior_b)?;
        let record = DisputeRecord { escalated, ..record };
        self.append(BridgeEvent::DisputeOpened { dispute: record.clone() }, now_unix_ms)?;

        if escalated {
            let candidates: Vec<&RegisteredProvider> = self.registry.all().collect();
            if let Some(auditor) = select_auditor(&record, &seed, beacon.observed_daa_score, &candidates)? {
                let dispute_id_hex = record.dispute_id_hex.clone();
                self.append(
                    BridgeEvent::DisputeAuditorSelected { dispute_id: dispute_id_hex.clone(), auditor },
                    now_unix_ms,
                )?;
                return Ok(Some(self.disputes[&dispute_id_hex].record.clone()));
            }
            // No unconflicted third party: the dispute stays open rather than being adjudicated
            // by someone with a stake in the answer.
        }
        Ok(Some(self.disputes[&record.dispute_id_hex].record.clone()))
    }

    /// Audit work waiting for this auditor (what an auditor polls for).
    pub fn audit_assignments_for(&self, auditor_bond: &str) -> Vec<(DisputeRecord, Vec<u32>, u32)> {
        self.disputes
            .values()
            .filter(|d| d.pending_auditor.as_deref() == Some(auditor_bond))
            .filter_map(|d| {
                let job = self.jobs.get(&d.record.job_id)?;
                Some((d.record.clone(), job.submission.prompt_ids.clone(), job.submission.max_new))
            })
            .collect()
    }

    /// The auditor's reference run resolves the dispute: attribute, name slash targets, journal
    /// the evidence.
    pub fn adjudicate_dispute(
        &mut self,
        dispute_id_hex: &str,
        auditor_bond: &str,
        reference_output_root: &str,
        reference_roots: &crate::wire::RuntimeRootsV1,
        now_unix_ms: i64,
    ) -> Result<SlashEvidenceV1, String> {
        let state = self.disputes.get(dispute_id_hex).ok_or_else(|| format!("unknown dispute {dispute_id_hex}"))?;
        if state.pending_auditor.as_deref() != Some(auditor_bond) {
            return Err(format!("dispute {dispute_id_hex} is not assigned to {auditor_bond}"));
        }
        let record = state.record.clone();
        let job = self.jobs.get(&record.job_id).ok_or("dispute job vanished")?;
        // Build the auditor's key through the SAME mapping both disputants went through, so the
        // reference output is comparable field-for-field.
        let reference_key = crate::match_key::build_match_key(
            &record.job_id,
            job.submission.max_new,
            &job.submission.prompt_ids,
            reference_output_root,
            reference_roots,
        )?;
        let (verdict, targets) = adjudicate(&record, &reference_key.output_commitment)?;
        let evidence = SlashEvidenceV1::new(
            &record,
            auditor_bond,
            &reference_key.output_commitment,
            verdict,
            targets,
            &self
                .leases
                .values()
                .next()
                .map(|l| l.beacon_seed_hex.clone())
                .unwrap_or_default(),
            &self.head_root_hex(),
        );
        let _ = verdict_str(verdict);
        self.append(BridgeEvent::DisputeAdjudicated { evidence: evidence.clone() }, now_unix_ms)?;
        Ok(evidence)
    }

    pub fn disputes_json(&self) -> Vec<serde_json::Value> {
        self.disputes.values().map(|d| serde_json::json!(d.record)).collect()
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
            job_challenge: None,
            output_token_ids: None,
            output_commitment: None,
        }
    }

    fn result(job: &str, provider: &str, output_root: &str, r: Option<RuntimeRootsV1>) -> ReplicaResultV1 {
        ReplicaResultV1 { job_id: job.into(), provider_id: provider.into(), output_root: output_root.into(), runtime_roots: r }
    }

    #[test]
    fn full_pipeline_and_independence() {
        let mut s = BridgeState::open(&dir("pipeline"), 120_000, 111).unwrap();
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
        let mut s = BridgeState::open(&dir("mismatch"), 120_000, 111).unwrap();
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
        let mut s = BridgeState::open(&dir("classreq"), 120_000, 111).unwrap();
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
        let mut s = BridgeState::open(&dir("requeue"), 1_000, 111).unwrap();
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
            let mut s = BridgeState::open(&d, 120_000, 111).unwrap();
            s.submit_job(&submission("j1", "prov-a"), 1).unwrap();
            let _ = s.fetch_assignments("prov-b", 2).unwrap();
            assert!(s.submit_replica_result(&result("j1", "prov-b", "dd44", Some(roots())), 3).unwrap());
            s.head_root_hex()
        };
        // Reopen: same head, same state — the matched job still delivers its verdicts.
        let mut s = BridgeState::open(&d, 120_000, 111).unwrap();
        assert_eq!(s.head_root_hex(), head);
        let v = s.fetch_verdicts(&["j1".into()], 10).unwrap();
        assert_eq!(v, vec![("j1".to_string(), JobVerdictV1::ReplicaMatched)]);

        // Torn final line (crash mid-append): dropped + truncated, the rest loads.
        let path = d.join("bridge-journal.jsonl");
        let mut bytes = std::fs::read(&path).unwrap();
        let intact = bytes.len();
        bytes.extend_from_slice(b"{\"seq\":999,\"torn");
        std::fs::write(&path, &bytes).unwrap();
        let s2 = BridgeState::open(&d, 120_000, 111).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), intact as u64, "torn tail truncated away");
        drop(s2);

        // Tamper INSIDE the chain: flip one byte of the first line ⇒ refuse to load.
        let mut bytes = std::fs::read(&path).unwrap();
        let flip = bytes.iter().position(|&b| b == b'1').unwrap();
        bytes[flip] = b'2';
        std::fs::write(&path, &bytes).unwrap();
        assert!(BridgeState::open(&d, 120_000, 111).is_err(), "altered journal must refuse to load");
    }
}
