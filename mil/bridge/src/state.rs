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

use crate::arbitration::{
    DisputeRecord, SlashEvidenceV1, adjudicate, dispute_id, is_escalated, select_auditor, select_replica, verdict_str,
};
use crate::chain::{ChainFacts, parse_outpoint};
use crate::challenge::{JobLeaseV1, request_commitment, salted_output_commitment};
use crate::da::{DaCommitmentWire, DaObligation, DaObligationStatus, DaResponseWire, register_obligations, timeout_evidence};
use crate::match_key::{hash64_hex, k2_match};
use crate::pcpb::{JobPreimage, PcpbProducedWitnessV1, PcpbSelfFlowRecordV1, PcpbSelfStepWire, external_witness};
use crate::provider::{ProviderRegistrationV1, ProviderRegistry, RegisteredProvider, SignedRequest};
use crate::wire::{JobSubmissionV1, JobVerdictV1, ReplicaAssignmentV1, ReplicaResultV1};

/// Keyed-BLAKE2b domain for journal chain roots (follows the `misaka-mil-v1/...` convention;
/// bridge-local so a journal root can never be replayed as any consensus value).
pub const BRIDGE_JOURNAL_DOMAIN: &[u8] = b"misaka-palw-bridge-v1/journal";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BridgeEvent {
    JobSubmitted {
        submission: JobSubmissionV1,
    },
    Assigned {
        job_id: String,
        provider_id: String,
        deadline_unix_ms: i64,
    },
    AssignmentLapsed {
        job_id: String,
        provider_id: String,
    },
    Declined {
        job_id: String,
        provider_id: String,
        reason: String,
    },
    ResultRecorded {
        result: ReplicaResultV1,
        matched: bool,
    },
    /// The `replica_matched` verdict was delivered to the submitter; the job is now certified
    /// (bridge-certified — see lib.rs for what that does NOT mean).
    CertifiedDelivered {
        job_id: String,
    },
    /// Seam 2: a bonded provider registered (already verified against the chain when accepted).
    ProviderRegistered {
        provider: RegisteredProvider,
    },
    /// Seam 1: a job challenge was leased against a buried beacon, before any generation.
    ChallengeLeased {
        lease: JobLeaseV1,
    },
    /// Seam 3: per-provider DA obligations registered for a job's context object.
    DaObligationsRegistered {
        obligations: Vec<DaObligation>,
    },
    DaChallenged {
        obligation_id: String,
        deadline_daa_score: u64,
    },
    DaSatisfied {
        obligation_id: String,
    },
    DaTimedOut {
        obligation_id: String,
        evidence_json: String,
    },
    /// Seam 4: a k=2 disagreement opened a dispute.
    DisputeOpened {
        dispute: DisputeRecord,
    },
    DisputeAuditorSelected {
        dispute_id: String,
        auditor: String,
    },
    DisputeAdjudicated {
        evidence: SlashEvidenceV1,
    },
    /// Seam 5 (ADR-0045 D3-b): a self-serial PCPB flow opened — A's commitment exists, the anchor
    /// may not. Journaled at open because an anchored commitment must survive a restart: forgetting
    /// it and re-anchoring under a fresh blind would re-roll B, the exact freedom the commit→draw
    /// ordering denies.
    PcpbSelfFlowOpened {
        flow: PcpbSelfFlowRecordV1,
    },
    /// Seam 5: the chain reported the anchor's registration epoch — the ordering fact B's draw
    /// hangs off. Recorded once; the flow's later steps re-derive from it.
    PcpbAnchorObserved {
        a_commit_hex: String,
        a_commit_epoch: u64,
    },
    /// Seam 5: a PCPB witness was produced (either branch), verified locally against the same
    /// checks consensus applies, and is now servable to the chunk builder.
    PcpbWitnessProduced {
        produced: PcpbProducedWitnessV1,
    },
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
    /// BRIDGE-SEL-01 — how many times this job's replica draw has been re-rolled (a lapse or a
    /// decline). Derived from the journal, so a restart re-computes the same selectee.
    assignment_round: u32,
}

/// A dispute plus the auditor assignment adjudicating it.
struct DisputeState {
    record: DisputeRecord,
    /// Set once an auditor is drawn; cleared when the auditor's reference run resolves it.
    pending_auditor: Option<String>,
}

/// Seam 5 — a self-serial PCPB flow and what the journal knows about its progress.
struct PcpbFlowState {
    record: PcpbSelfFlowRecordV1,
    a_commit_epoch: Option<u64>,
    /// The leaf challenge of the produced witness, once `finish` succeeded (terminal state).
    produced_challenge_hex: Option<String>,
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
    /// Seam 5 — self-serial PCPB flows by `a_commit` hex.
    pcpb_flows: BTreeMap<String, PcpbFlowState>,
    /// Seam 5 — produced witnesses by LEAF challenge hex (the consensus `receipt_v3_job_challenge`).
    pcpb_witnesses: BTreeMap<String, PcpbProducedWitnessV1>,
    /// Per-credential mismatch history feeding the repeat-offender rule.
    mismatch_counts: BTreeMap<String, u32>,
    /// Spent request nonces with the expiry that retires them, keyed `(bond, nonce)`. In-memory
    /// by design: a nonce is only meaningful inside its own acceptance window, and a bridge
    /// restart moves `now` past every window it was holding.
    spent_nonces: BTreeMap<(String, String), i64>,
    network_id: u32,
}

/// How far in the future a request signature may claim to expire. Bounds how long one captured
/// signature stays replayable and how large the nonce set can grow.
pub const MAX_REQUEST_LIFETIME_MS: i64 = 120_000;

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
            pcpb_flows: BTreeMap::new(),
            pcpb_witnesses: BTreeMap::new(),
            mismatch_counts: BTreeMap::new(),
            spent_nonces: BTreeMap::new(),
            network_id,
        };

        // Replay + verify. A missing trailing newline marks a torn final append: drop it and
        // truncate the file back to the last good byte so the next append starts clean.
        let mut valid_len = 0usize;
        let mut cursor = 0usize;
        while cursor < existing.len() {
            let Some(nl) = existing[cursor..].iter().position(|&b| b == b'\n') else {
                eprintln!("[palw-bridge] dropping torn final journal line ({} bytes) — crash recovery", existing.len() - cursor);
                break;
            };
            let line = &existing[cursor..cursor + nl];
            let parsed: JournalLine = serde_json::from_slice(line).map_err(|e| format!("journal line {}: {e}", state.seq + 1))?;
            if parsed.seq != state.seq + 1 {
                return Err(format!("journal line {}: sequence says {} — chain broken", state.seq + 1, parsed.seq));
            }
            let event_json = serde_json::to_vec(&parsed.event).map_err(|e| e.to_string())?;
            let expect = chain_root(&state.head_root, &event_json);
            if hash64_hex(&expect) != parsed.root {
                return Err(format!("journal line {}: chain root mismatch — the journal was altered; refusing to load", parsed.seq));
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
                    JobRecord {
                        submission: submission.clone(),
                        phase: Phase::Unassigned,
                        declines: 0,
                        result: None,
                        assignment_round: 0,
                    },
                );
            }
            BridgeEvent::Assigned { job_id, provider_id, deadline_unix_ms } => {
                if let Some(job) = self.jobs.get_mut(job_id) {
                    job.phase = Phase::Assigned { provider_id: provider_id.clone(), deadline_unix_ms: *deadline_unix_ms };
                }
            }
            BridgeEvent::AssignmentLapsed { job_id, .. } => {
                if let Some(job) = self.jobs.get_mut(job_id) {
                    job.phase = Phase::Unassigned;
                    // Re-roll: the next draw must not return the selectee that just went silent.
                    job.assignment_round = job.assignment_round.saturating_add(1);
                }
            }
            BridgeEvent::Declined { job_id, provider_id, .. } => {
                if let Some(job) = self.jobs.get_mut(job_id) {
                    job.phase = Phase::Unassigned;
                    job.declines += 1;
                    job.assignment_round = job.assignment_round.saturating_add(1);
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
                self.disputes.insert(dispute.dispute_id_hex.clone(), DisputeState { record: dispute.clone(), pending_auditor: None });
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
            BridgeEvent::PcpbSelfFlowOpened { flow } => {
                self.pcpb_flows.insert(
                    flow.a_commit_hex.clone(),
                    PcpbFlowState { record: flow.clone(), a_commit_epoch: None, produced_challenge_hex: None },
                );
            }
            BridgeEvent::PcpbAnchorObserved { a_commit_hex, a_commit_epoch } => {
                if let Some(flow) = self.pcpb_flows.get_mut(a_commit_hex) {
                    flow.a_commit_epoch = Some(*a_commit_epoch);
                }
            }
            BridgeEvent::PcpbWitnessProduced { produced } => {
                if produced.dispatch_kind == kaspa_consensus_core::palw::PALW_DISPATCH_KIND_SELF_SERIAL
                    && let Some(flow) = self.pcpb_flows.get_mut(&produced.a_commit_hex)
                {
                    flow.produced_challenge_hex = Some(produced.leaf_challenge_hex.clone());
                }
                self.pcpb_witnesses.insert(produced.leaf_challenge_hex.clone(), produced.clone());
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
            return Err("runtime_roots required: this bridge coordinates the qi35-serve class, whose match key covers \
                 the engine execution roots (update the gateway if it predates ROOTS capture)"
                .into());
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

    pub fn fetch_verdicts(&mut self, job_ids: &[String], now_unix_ms: i64) -> Result<Vec<(String, JobVerdictV1)>, String> {
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
        chain: Option<&dyn ChainFacts>,
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
        //
        // BRIDGE-SEL-01: with a chain, the replica is DERIVED from the beacon over the frozen
        // bond set, so this route reports the draw rather than granting the job to whoever asked
        // first. Without a chain the bridge is the dev harness (`require_bonded == false`, no
        // signatures either) and keeps claim-on-fetch — there is no beacon to draw against, and
        // that mode is not a security posture.
        let unassigned: Vec<String> = self
            .jobs
            .iter()
            .filter(|(_, job)| job.phase == Phase::Unassigned && job.submission.provider_id != provider_id)
            .map(|(id, _)| id.clone())
            .collect();
        let offer: Vec<String> = match chain {
            None => unassigned,
            Some(chain) => {
                let beacon = chain.beacon()?;
                let seed = beacon.seed()?;
                let candidates: Vec<&RegisteredProvider> = self.registry.all().collect();
                let mut drawn = Vec::new();
                for job_id in unassigned {
                    let job = &self.jobs[&job_id];
                    let selected = select_replica(
                        &job_id,
                        &job.submission.provider_id,
                        &seed,
                        beacon.observed_daa_score,
                        job.assignment_round,
                        &candidates,
                    )?;
                    // Only the drawn provider is told about the job at all: a non-selectee polling
                    // this route learns nothing and can claim nothing.
                    if selected.as_deref() == Some(provider_id) {
                        drawn.push(job_id);
                    }
                }
                drawn
            }
        };
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

    pub fn decline_assignment(&mut self, job_id: &str, provider_id: &str, reason: &str, now_unix_ms: i64) -> Result<(), String> {
        let Some(job) = self.jobs.get(job_id) else { return Err(format!("unknown job {job_id}")) };
        match &job.phase {
            Phase::Assigned { provider_id: holder, .. } if holder == provider_id => self.append(
                BridgeEvent::Declined { job_id: job_id.to_string(), provider_id: provider_id.to_string(), reason: reason.to_string() },
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
                    self.append(BridgeEvent::AssignmentLapsed { job_id: result.job_id.clone(), provider_id: holder }, now_unix_ms)?;
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
    /// Verify one signed request: the signature over the WHOLE request, then freshness.
    ///
    /// Takes `&mut self` because accepting a request spends its nonce — a valid signature is
    /// usable exactly once, so an observer who captures a request cannot repeat it. Nonce and
    /// expiry are checked only AFTER the signature verifies, so an unauthenticated caller cannot
    /// burn a nonce it does not own.
    pub fn authenticate(
        &mut self,
        request: &SignedRequest<'_>,
        signature_hex: &str,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<&RegisteredProvider, String> {
        let beacon = chain.beacon()?;
        self.registry.authenticate(request, signature_hex, chain, beacon.current_epoch)?;
        if request.expires_at_unix_ms <= now_unix_ms {
            return Err("request signature has expired".into());
        }
        if request.expires_at_unix_ms > now_unix_ms + MAX_REQUEST_LIFETIME_MS {
            return Err(format!("request expiry is more than {MAX_REQUEST_LIFETIME_MS} ms in the future"));
        }
        if request.nonce.is_empty() {
            return Err("request nonce must not be empty".into());
        }
        self.spent_nonces.retain(|_, expires| *expires > now_unix_ms);
        let key = (request.bond_outpoint.to_string(), request.nonce.to_string());
        if self.spent_nonces.contains_key(&key) {
            return Err("request nonce has already been used (replay)".into());
        }
        self.spent_nonces.insert(key, request.expires_at_unix_ms);
        // Re-borrow immutably now that the nonce is spent.
        self.registry.get(request.bond_outpoint).ok_or_else(|| "provider vanished mid-authentication".to_string())
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
        let scheduler_job_id = blake2b_512_keyed(b"misaka-palw-bridge-v1/scheduler-job-id", &{
            let mut preimage = Vec::with_capacity(136);
            preimage.extend_from_slice(credential.as_byte_slice());
            preimage.extend_from_slice(commitment.as_byte_slice());
            preimage.extend_from_slice(&beacon.epoch.to_le_bytes());
            preimage
        });
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
        let lease =
            self.lease_for_challenge(challenge_hex).ok_or("job_challenge was not issued by this bridge (or has been forgotten)")?;
        let provider = self.registry.get(requester_bond).ok_or_else(|| format!("provider {requester_bond} is not registered"))?;
        lease.accepts(&submission.prompt_ids, submission.max_new, class_label, &provider.credential()?, current_epoch)?;

        let Some(output_ids) = &submission.output_token_ids else {
            return Err("output_token_ids required for the salted output commitment".into());
        };
        let expected = salted_output_commitment(output_ids, &lease.job_challenge()?);
        let claimed = submission.output_commitment.as_deref().ok_or("output_commitment required (receipt-v3 salted commitment)")?;
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
    pub fn answer_da_challenge(&mut self, response: &DaResponseWire, chain: &dyn ChainFacts, now_unix_ms: i64) -> Result<(), String> {
        let obligation = self
            .obligations
            .get(&response.obligation_id_hex)
            .ok_or_else(|| format!("unknown obligation {}", response.obligation_id_hex))?
            .clone();
        if obligation.provider_bond != response.provider_bond {
            return Err("response comes from a different provider than the obligation".into());
        }
        // Verify FIRST, before any status short-circuit. A response for an already-satisfied
        // obligation is idempotent — but only if it actually proves the chunk. Short-circuiting
        // on status would report success for a proof that was never checked, which is a lie the
        // caller cannot distinguish from a real verification.
        response.verify(&obligation)?;
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
            self.append(BridgeEvent::DaTimedOut { obligation_id: obligation.obligation_id_hex.clone(), evidence_json }, now_unix_ms)?;
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
    pub fn open_dispute(&mut self, job_id: &str, chain: &dyn ChainFacts, now_unix_ms: i64) -> Result<Option<DisputeRecord>, String> {
        let job = self.jobs.get(job_id).ok_or_else(|| format!("unknown job {job_id}"))?;
        let Some(result) = &job.result else { return Err("job has no replica result to dispute".into()) };
        let submitter = job.submission.provider_id.clone();
        let replica = result.provider_id.clone();
        let (Some(a_roots), Some(b_roots)) = (&job.submission.runtime_roots, &result.runtime_roots) else {
            return Err("both sides must carry runtime roots to open a dispute".into());
        };
        // The k=2 field that differs is the output commitment; carry each side's own.
        let a_key = crate::match_key::build_match_key(
            job_id,
            job.submission.max_new,
            &job.submission.prompt_ids,
            &job.submission.output_root,
            a_roots,
        )?;
        let b_key = crate::match_key::build_match_key(
            job_id,
            job.submission.max_new,
            &job.submission.prompt_ids,
            &result.output_root,
            b_roots,
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
                self.append(BridgeEvent::DisputeAuditorSelected { dispute_id: dispute_id_hex.clone(), auditor }, now_unix_ms)?;
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
            &self.leases.values().next().map(|l| l.beacon_seed_hex.clone()).unwrap_or_default(),
            &self.head_root_hex(),
        );
        let _ = verdict_str(verdict);
        self.append(BridgeEvent::DisputeAdjudicated { evidence: evidence.clone() }, now_unix_ms)?;
        Ok(evidence)
    }

    pub fn disputes_json(&self) -> Vec<serde_json::Value> {
        self.disputes.values().map(|d| serde_json::json!(d.record)).collect()
    }

    // ---- seam 5: PCPB evidence production (ADR-0045 D3-b) -----------------------------------

    /// The bond outpoints of every provider registered with this bridge — the resolution set for
    /// drawn provider ids (`palw_provider_id` is one-way, so an id is only routable if we know its
    /// preimage outpoint). The honest-maximum caveat from `chain.rs` applies: this is the bridge's
    /// view, not the chain registry.
    fn registry_bonds(&self) -> Vec<kaspa_consensus_core::tx::TransactionOutpoint> {
        self.registry.all().filter_map(|p| p.outpoint().ok()).collect()
    }

    /// Open (idempotently) a self-serial PCPB flow and report its current step.
    ///
    /// The record is journaled BEFORE the anchor exists so a crash between "submit the `0x45`" and
    /// "observe its epoch" cannot orphan an on-chain anchor. Re-opening with the identical record
    /// is a no-op returning the current step; a DIFFERENT record under the same `a_commit` is
    /// refused (a commitment is an identity, not a slot).
    pub fn open_pcpb_self_flow(
        &mut self,
        record: &PcpbSelfFlowRecordV1,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<PcpbSelfStepWire, String> {
        record.to_flow(None)?; // every hex field parses, or THIS request fails, not a later step
        if self.registry.get(&record.a_bond).is_none() {
            return Err(format!("provider {} is not registered with this bridge (self-order is a bonded privilege)", record.a_bond));
        }
        match self.pcpb_flows.get(&record.a_commit_hex) {
            Some(existing) if existing.record == *record => {}
            Some(_) => return Err(format!("PCPB flow {} already exists with different content", record.a_commit_hex)),
            None => self.append(BridgeEvent::PcpbSelfFlowOpened { flow: record.clone() }, now_unix_ms)?,
        }
        self.drive_pcpb_self_flow(&record.a_commit_hex, &record.a_bond, chain, now_unix_ms)
    }

    /// Advance a self-serial flow as far as the chain currently allows and say what it waits for.
    ///
    /// Every step is re-derived from journal + chain; nothing is guessed. The anchor's registration
    /// epoch is read back from the node (never declared by the caller) and journaled on first
    /// observation — the leaf will name it, and clause 12 holds the leaf to the registry row.
    pub fn drive_pcpb_self_flow(
        &mut self,
        a_commit_hex: &str,
        caller_bond: &str,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<PcpbSelfStepWire, String> {
        let state = self.pcpb_flows.get(a_commit_hex).ok_or_else(|| format!("unknown PCPB flow {a_commit_hex}"))?;
        if state.record.a_bond != caller_bond {
            return Err("PCPB flow belongs to a different provider bond".into());
        }
        if let (Some(epoch), Some(challenge)) = (state.a_commit_epoch, &state.produced_challenge_hex) {
            return Ok(PcpbSelfStepWire::Ready { a_commit_epoch: epoch, leaf_challenge_hex: challenge.clone() });
        }
        let record = state.record.clone();
        let known_epoch = state.a_commit_epoch;
        let a_commit = crate::chain::parse_hash64(a_commit_hex)?;

        // One chain query at the epoch we know; a probe (epoch 0) when we do not. The probe cannot
        // yield a buildable context, but it DOES report the registry row — the fact we are after.
        let (ctx, observed) = chain.pcpb_context(known_epoch.unwrap_or(0), Some(a_commit))?;
        let anchored = match (known_epoch, observed) {
            (Some(epoch), _) => Some(epoch),
            (None, Some(epoch)) => {
                self.append(
                    BridgeEvent::PcpbAnchorObserved { a_commit_hex: a_commit_hex.to_string(), a_commit_epoch: epoch },
                    now_unix_ms,
                )?;
                Some(epoch)
            }
            (None, None) => None,
        };
        // The probe's context was for epoch 0; once the real anchor epoch is known, re-resolve there.
        let ctx = match (known_epoch, anchored) {
            (Some(_), _) => ctx,
            (None, Some(epoch)) => chain.pcpb_context(epoch, Some(a_commit))?.0,
            (None, None) => None,
        };
        let flow = record.to_flow(anchored)?;
        let step = flow.step(ctx.as_ref(), &self.registry_bonds()).map_err(|e| e.to_string())?;
        Ok(PcpbSelfStepWire::from_step(&step, anchored))
    }

    /// Assemble the self-serial witness from B's signed receipt.
    ///
    /// `finish` re-runs every check consensus will make (drawn B, committed key hash, `a_commit`
    /// embedding, real ML-DSA-87 verify), so a bad partner is refused HERE, before the leaf exists
    /// and before any chunk fee — clause 12's rejection at acceptance would be silent.
    pub fn pcpb_partner_receipt(
        &mut self,
        a_commit_hex: &str,
        caller_bond: &str,
        b_ml_dsa_pk_hex: &str,
        b_receipt_preimage_hex: &str,
        b_signature_hex: &str,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<PcpbProducedWitnessV1, String> {
        let state = self.pcpb_flows.get(a_commit_hex).ok_or_else(|| format!("unknown PCPB flow {a_commit_hex}"))?;
        if state.record.a_bond != caller_bond {
            return Err("PCPB flow belongs to a different provider bond".into());
        }
        if let Some(challenge) = &state.produced_challenge_hex {
            // Terminal: the witness exists; a re-post cannot re-roll anything.
            return Ok(self.pcpb_witnesses[challenge].clone());
        }
        let a_commit_epoch = state.a_commit_epoch.ok_or("the A-commit anchor is not observed on-chain yet — keep polling the flow")?;
        let record = state.record.clone();
        let a_commit = crate::chain::parse_hash64(a_commit_hex)?;
        let (ctx, _) = chain.pcpb_context(a_commit_epoch, Some(a_commit))?;
        let ctx = ctx.ok_or("the node cannot resolve this anchor's PCPB context yet (draw beacon open, or window aged out)")?;
        let flow = record.to_flow(Some(a_commit_epoch))?;
        let produced = flow
            .finish(
                &ctx,
                crate::match_key::decode_hex(b_ml_dsa_pk_hex)?,
                crate::match_key::decode_hex(b_receipt_preimage_hex)?,
                crate::match_key::decode_hex(b_signature_hex)?,
                &self.registry_bonds(),
                self.network_id,
            )
            .map_err(|e| e.to_string())?;
        let wire = PcpbProducedWitnessV1::from_produced(&produced, None);
        self.append(BridgeEvent::PcpbWitnessProduced { produced: wire.clone() }, now_unix_ms)?;
        Ok(wire)
    }

    /// External-branch witness production, driven off a Seam-1 lease.
    ///
    /// The lease IS the challenge issuance: its preimage triple, shape and epoch are the exact
    /// inputs clause 11 re-derives from, and its anchor epoch is where the pair is drawn. The
    /// caller must be the lease's requester. The node's `R_anchor` must equal the seed the lease
    /// was issued under — a disagreement means bridge and node see different beacon histories, and
    /// evidence built on either would be refused by the other's verifier.
    pub fn produce_pcpb_external_witness(
        &mut self,
        lease_challenge_hex: &str,
        caller_bond: &str,
        chain: &dyn ChainFacts,
        now_unix_ms: i64,
    ) -> Result<PcpbProducedWitnessV1, String> {
        let lease = self
            .lease_for_challenge(lease_challenge_hex)
            .ok_or("job_challenge was not issued by this bridge (or has been forgotten)")?
            .clone();
        let provider =
            self.registry.get(caller_bond).ok_or_else(|| format!("provider {caller_bond} is not registered with this bridge"))?;
        if hash64_hex(&provider.credential()?) != lease.requester_credential_hex {
            return Err("lease belongs to a different requester credential".into());
        }
        // The consensus challenge derivation is BYTE-IDENTICAL to Seam 1's (the D3-b slice promoted
        // the bridge recipe, domain string included, so issued leases keep verifying) — the lease
        // challenge IS the leaf challenge, and the produced-witness map is keyed by it directly.
        if let Some(existing) = self.pcpb_witnesses.get(lease_challenge_hex) {
            return Ok(existing.clone());
        }
        let (ctx, _) = chain.pcpb_context(lease.beacon_epoch, None)?;
        let ctx = ctx.ok_or_else(|| {
            format!(
                "the node cannot serve the PCPB context for anchor epoch {} yet (draw beacon open, or window aged out)",
                lease.beacon_epoch
            )
        })?;
        if hash64_hex(&ctx.anchor_seed) != lease.beacon_seed_hex {
            return Err("node's R_anchor differs from the lease's beacon seed — refusing to build divergent evidence".into());
        }
        let preimage = JobPreimage {
            scheduler_job_id: crate::chain::parse_hash64(&lease.scheduler_job_id_hex)?,
            requester_credential: crate::chain::parse_hash64(&lease.requester_credential_hex)?,
            request_commitment: crate::chain::parse_hash64(&lease.request_commitment_hex)?,
        };
        let produced =
            external_witness(&ctx, lease.network_id, preimage, lease.shape_id, &self.registry_bonds()).map_err(|e| e.to_string())?;
        let wire = PcpbProducedWitnessV1::from_produced(&produced, Some(lease_challenge_hex.to_string()));
        self.append(BridgeEvent::PcpbWitnessProduced { produced: wire.clone() }, now_unix_ms)?;
        Ok(wire)
    }

    /// A produced witness by its LEAF challenge (what the leaf's `receipt_v3_job_challenge` will be).
    pub fn pcpb_witness(&self, leaf_challenge_hex: &str) -> Option<&PcpbProducedWitnessV1> {
        self.pcpb_witnesses.get(leaf_challenge_hex)
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
            "pcpb": {
                "self_flows": self.pcpb_flows.len(),
                "anchored_flows": self.pcpb_flows.values().filter(|f| f.a_commit_epoch.is_some()).count(),
                "witnesses": self.pcpb_witnesses.len(),
            },
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
        assert!(s.fetch_assignments("prov-a", None, 2_000).unwrap().is_empty());

        let got = s.fetch_assignments("prov-b", None, 2_000).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].prompt_ids, vec![1, 2, 3]);
        assert!(s.fetch_assignments("prov-b", None, 2_001).unwrap().is_empty(), "claimed once");
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
        let _ = s.fetch_assignments("prov-b", None, 2).unwrap();
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
        let _ = s.fetch_assignments("prov-b", None, 3).unwrap();
        // Result without roots: refused (class failure), NOT a mismatch — the job requeues via
        // deadline lapse instead of branding the submitter's turn.
        assert!(s.submit_replica_result(&result("j2", "prov-b", "dd44", None), 4).is_err());
        assert!(s.fetch_verdicts(&["j2".into()], 5).unwrap().is_empty(), "no verdict was reached");
    }

    #[test]
    fn decline_and_deadline_requeue() {
        let mut s = BridgeState::open(&dir("requeue"), 1_000, 111).unwrap();
        s.submit_job(&submission("j1", "prov-a"), 1_000).unwrap();
        let got = s.fetch_assignments("prov-b", None, 1_000).unwrap();
        assert_eq!(got.len(), 1);
        s.decline_assignment("j1", "prov-b", "over capacity", 1_100).unwrap();
        // Requeued: another provider claims it.
        let got = s.fetch_assignments("prov-c", None, 1_200).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].deadline_unix_ms, 2_200);
        // Silent past the deadline ⇒ lapse on the next poll; then a late result is refused.
        let got = s.fetch_assignments("prov-d", None, 5_000).unwrap();
        assert_eq!(got.len(), 1, "lapsed claim re-offered");
        assert!(s.submit_replica_result(&result("j1", "prov-c", "dd44", Some(roots())), 5_100).is_err());
    }

    #[test]
    fn journal_replay_restores_state_and_detects_tamper() {
        let d = dir("durability");
        let head = {
            let mut s = BridgeState::open(&d, 120_000, 111).unwrap();
            s.submit_job(&submission("j1", "prov-a"), 1).unwrap();
            let _ = s.fetch_assignments("prov-b", None, 2).unwrap();
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

    // ---- seam 5: PCPB production over the journal + a pinned chain ---------------------------

    mod pcpb {
        use super::*;
        use crate::chain::{BeaconFacts, BondFacts, PinnedChainFacts, PinnedFactsFile, PinnedPcpbAnchor, PinnedPcpbEntry};
        use crate::pcpb::{PcpbSelfFlowRecordV1, PcpbSelfStepWire};
        use kaspa_consensus_core::palw::{
            PALW_DISPATCH_KIND_BEACON_ASSIGNED, PALW_DISPATCH_KIND_SELF_SERIAL, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT,
            PalwDispatchLeafFacts, PalwMlDsaVerifier, palw_build_snapshot_witnesses, palw_dispatch_evidence_valid, palw_pcpb_derive_b,
            palw_provider_id, palw_provider_pk_hash,
        };
        use kaspa_consensus_core::tx::TransactionOutpoint;
        use kaspa_hashes::Hash64;
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        /// The production verifier binding, so the produced witnesses are judged by the REAL
        /// clause-12 predicate rather than a stub that could agree with a wiring bug.
        struct TxscriptVerifier;
        impl PalwMlDsaVerifier for TxscriptVerifier {
            fn verify(&self, vk: &[u8], msg: &[u8], ctx: &[u8], sig: &[u8]) -> bool {
                matches!(kaspa_txscript::verify_mldsa87_with_context(vk, msg, sig, ctx), Ok(true))
            }
        }

        fn th(b: u8) -> Hash64 {
            Hash64::from_bytes([b; 64])
        }

        const ANCHOR_EPOCH: u64 = 42;

        struct PcpbEnv {
            /// Facts BEFORE the anchor is on-chain (no registry row, draw open).
            unanchored: PinnedChainFacts,
            /// Facts with the anchor registered but the draw beacon still open.
            anchored_waiting: PinnedChainFacts,
            /// Facts with the full context (registered anchor + closed draw beacon).
            complete: PinnedChainFacts,
            bonds: Vec<TransactionOutpoint>,
            keys: Vec<mldsa::MLDSA87KeyPair>,
            beacon_seed: Hash64,
            draw_seed: Hash64,
        }

        /// Four REAL-keyed bonded providers pinned at `ANCHOR_EPOCH`, in the three phases a
        /// self-serial flow traverses. The draw seed is searched so the external pair is distinct
        /// (exactly what an honest scheduler waits for).
        fn pcpb_env(a_commit_hex: &str) -> PcpbEnv {
            let beacon_seed = th(0x11);
            let mut keys = Vec::new();
            let mut bonds = Vec::new();
            let mut entries = Vec::new();
            for i in 0..4u8 {
                let kp = mldsa::generate_key_pair([i + 1; 32]);
                let outpoint = TransactionOutpoint::new(th(0xB0 + i), i as u32);
                entries.push(PinnedPcpbEntry {
                    provider_id_hex: hash64_hex(&palw_provider_id(&outpoint)),
                    ml_dsa_pk_hash_hex: hash64_hex(&palw_provider_pk_hash(kp.verification_key.as_ref())),
                    bond_sompi: 250,
                    reward_script_commitment_hex: hash64_hex(&th(0xD0 + i)),
                });
                bonds.push(outpoint);
                keys.push(kp);
            }
            // Find a draw seed whose two external slots are distinct providers.
            let consensus_entries: Vec<_> = entries
                .iter()
                .zip(&bonds)
                .map(|(e, bond)| kaspa_consensus_core::palw::PalwProviderSnapshotEntry {
                    provider_id: palw_provider_id(bond),
                    ml_dsa_pk_hash: crate::chain::parse_hash64(&e.ml_dsa_pk_hash_hex).unwrap(),
                    bond_sompi: e.bond_sompi,
                    reward_script_commitment: crate::chain::parse_hash64(&e.reward_script_commitment_hex).unwrap(),
                })
                .collect();
            let witnesses = palw_build_snapshot_witnesses(&consensus_entries);
            let draw_seed = (0..64u8)
                .map(|k| th(0x40u8.wrapping_add(k)))
                .find(|seed| {
                    let a = witnesses.select(&kaspa_consensus_core::palw::palw_assignment_draw_seed(seed, 0));
                    let b = witnesses.select(&kaspa_consensus_core::palw::palw_assignment_draw_seed(seed, 1));
                    matches!((a, b), (Some(a), Some(b)) if a != b)
                })
                .expect("some seed draws a distinct external pair");

            let beacon = BeaconFacts {
                epoch: ANCHOR_EPOCH,
                seed_hex: hash64_hex(&beacon_seed),
                anchor_hash_hex: "cd".repeat(64),
                anchor_daa_score: ANCHOR_EPOCH * 100,
                observed_daa_score: ANCHOR_EPOCH * 100 + 300,
                current_epoch: ANCHOR_EPOCH + 1,
            };
            let anchor = PinnedPcpbAnchor {
                snapshot_epoch: ANCHOR_EPOCH - 2,
                draw_epoch: ANCHOR_EPOCH + 2,
                entries,
                anchor_seed_hex: hash64_hex(&beacon_seed),
                draw_seed_hex: Some(hash64_hex(&draw_seed)),
            };
            let mut waiting_anchor = anchor.clone();
            waiting_anchor.draw_seed_hex = None;

            let facts = |anchor: Option<PinnedPcpbAnchor>, acommit: Option<(&str, u64)>| {
                let mut file = PinnedFactsFile {
                    beacon: beacon.clone(),
                    bonds: BTreeMap::new(),
                    pcpb_anchors: BTreeMap::new(),
                    pcpb_acommits: BTreeMap::new(),
                };
                if let Some(a) = anchor {
                    file.pcpb_anchors.insert(ANCHOR_EPOCH, a);
                }
                if let Some((commit, epoch)) = acommit {
                    file.pcpb_acommits.insert(commit.to_string(), epoch);
                }
                PinnedChainFacts::from_facts(file)
            };
            PcpbEnv {
                unanchored: facts(Some(anchor.clone()), None),
                anchored_waiting: facts(Some(waiting_anchor), Some((a_commit_hex, ANCHOR_EPOCH))),
                complete: facts(Some(anchor), Some((a_commit_hex, ANCHOR_EPOCH))),
                bonds,
                keys,
                beacon_seed,
                draw_seed,
            }
        }

        /// Register the env's providers straight through the journal (the verification half of
        /// registration is Seam 2's own test surface; Seam 5 needs the registry CONTENTS).
        fn register(s: &mut BridgeState, env: &PcpbEnv) {
            for (i, bond) in env.bonds.iter().enumerate() {
                let outpoint = crate::chain::format_outpoint(bond);
                let provider = RegisteredProvider {
                    bond_outpoint: outpoint.clone(),
                    owner_public_key_hex: bytes_hex_local(env.keys[i].verification_key.as_ref()),
                    credential_hex: hash64_hex(&th(0x70 + i as u8)),
                    session_public_key_hex: String::new(),
                    session_valid_from_epoch: 0,
                    session_valid_until_epoch: u64::MAX,
                    bond: BondFacts {
                        bond_outpoint: outpoint,
                        owner_pubkey_hash_hex: hash64_hex(&th(0x70 + i as u8)),
                        operator_group_id_hex: hash64_hex(&th(0x60 + i as u8)),
                        amount_sompi: 250,
                        activation_daa_score: 0,
                        effective_status: "active".into(),
                        unbond_request_daa_score: None,
                        slashed_at_daa_score: None,
                        unbond_delay_epochs: 6,
                        reward_key_root_hex: hash64_hex(&th(0xD0 + i as u8)),
                        runtime_classes_hex: vec![],
                        capacity_by_shape: vec![(1, 4)],
                    },
                };
                s.append(BridgeEvent::ProviderRegistered { provider }, 1).unwrap();
            }
        }

        fn bytes_hex_local(bytes: &[u8]) -> String {
            crate::match_key::bytes_hex(bytes)
        }

        fn facts_of(w: &crate::pcpb::PcpbProducedWitnessV1) -> PalwDispatchLeafFacts {
            PalwDispatchLeafFacts {
                snapshot_root: crate::chain::parse_hash64(&w.provider_snapshot_root_hex).unwrap(),
                assignment_root: crate::chain::parse_hash64(&w.assignment_proof_root_hex).unwrap(),
                a_commit: crate::chain::parse_hash64(&w.a_commit_hex).unwrap(),
                dispatch_kind: w.dispatch_kind,
                provider_a_id: palw_provider_id(&crate::chain::parse_outpoint(&w.provider_a_bond).unwrap()),
                provider_b_id: palw_provider_id(&crate::chain::parse_outpoint(&w.provider_b_bond).unwrap()),
            }
        }

        /// The full self-serial lifecycle over the HTTP-facing state methods: open → submit-anchor
        /// step → anchor observed (journaled) → draw wait → partner receipt → witness that the REAL
        /// clause-12 verifier accepts — and every stage of it survives a bridge restart.
        #[test]
        fn pcpb_self_flow_produces_and_survives_restart() {
            let commitment = crate::pcpb::a_commit(b"job-descriptor", &th(0x77), &[0x33; 32]);
            let a_commit_hex = hash64_hex(&commitment);
            let env = pcpb_env(&a_commit_hex);
            let d = dir("pcpb-self");
            let mut s = BridgeState::open(&d, 120_000, 110).unwrap();
            register(&mut s, &env);

            // Pick A's seat knowing who the post-commit draw selects for B — a real A whose blind
            // drew itself would re-anchor; the fixture simply avoids that seat up front.
            let consensus_entries: Vec<_> = env
                .bonds
                .iter()
                .enumerate()
                .map(|(i, bond)| kaspa_consensus_core::palw::PalwProviderSnapshotEntry {
                    provider_id: palw_provider_id(bond),
                    ml_dsa_pk_hash: palw_provider_pk_hash(env.keys[i].verification_key.as_ref()),
                    bond_sompi: 250,
                    reward_script_commitment: th(0xD0 + i as u8),
                })
                .collect();
            let ws = palw_build_snapshot_witnesses(&consensus_entries);
            let drawn_b_id = ws.slots[ws.select(&palw_pcpb_derive_b(&env.draw_seed, &commitment)).unwrap()].entry.provider_id;
            let a_seat = (0..4).find(|&i| palw_provider_id(&env.bonds[i]) != drawn_b_id).unwrap();

            let record = PcpbSelfFlowRecordV1 {
                a_commit_hex: a_commit_hex.clone(),
                a_bond: crate::chain::format_outpoint(&env.bonds[a_seat]),
                scheduler_job_id_hex: hash64_hex(&th(0xE1)),
                requester_credential_hex: hash64_hex(&th(0xE2)),
                request_commitment_hex: hash64_hex(&th(0xE3)),
                shape_id: 1,
                receipt_tail_hex: bytes_hex_local(b"self-tail"),
            };

            // (1) Unanchored: the step is "submit this 0x45 payload" and the payload validates.
            let step = s.open_pcpb_self_flow(&record, &env.unanchored, 10).unwrap();
            let PcpbSelfStepWire::SubmitAnchor { subnetwork_byte, payload_hex } = step else {
                panic!("expected SubmitAnchor, got {step:?}");
            };
            assert_eq!(subnetwork_byte, 0x45);
            let payload = crate::match_key::decode_hex(&payload_hex).unwrap();
            assert_eq!(kaspa_consensus_core::palw::validate_palw_acommit_tx(&payload), Ok(()));
            // Re-opening with the identical record is a no-op; different content is refused.
            s.open_pcpb_self_flow(&record, &env.unanchored, 11).unwrap();
            let mut different = record.clone();
            different.receipt_tail_hex = bytes_hex_local(b"other-tail");
            assert!(s.open_pcpb_self_flow(&different, &env.unanchored, 12).unwrap_err().contains("different content"));

            // (2) Anchor registered, draw beacon open: the flow WAITS (and journals the epoch).
            // The pinned "waiting" world serves NO buildable context, so the wire reports the wait
            // against the anchor epoch itself (there is no resolved draw epoch to name yet).
            let step = s.drive_pcpb_self_flow(&a_commit_hex, &record.a_bond, &env.anchored_waiting, 20).unwrap();
            assert_eq!(step, PcpbSelfStepWire::AwaitDrawBeacon { a_commit_epoch: ANCHOR_EPOCH, draw_epoch: ANCHOR_EPOCH });
            // A foreign bond cannot read the flow.
            let foreign = crate::chain::format_outpoint(&env.bonds[(a_seat + 1) % 4]);
            assert!(s.drive_pcpb_self_flow(&a_commit_hex, &foreign, &env.anchored_waiting, 21).is_err());

            // (3) Draw closed: the flow names B and the exact bytes to sign — a REAL bond outpoint,
            // resolved through the same provider-id derivation consensus applies.
            let step = s.drive_pcpb_self_flow(&a_commit_hex, &record.a_bond, &env.complete, 30).unwrap();
            let PcpbSelfStepWire::AwaitPartnerReceipt { a_commit_epoch, partner_bond, receipt_preimage_hex } = step else {
                panic!("expected AwaitPartnerReceipt, got {step:?}");
            };
            assert_eq!(a_commit_epoch, ANCHOR_EPOCH);
            let b_id = palw_provider_id(&crate::chain::parse_outpoint(&partner_bond).unwrap());
            assert_eq!(b_id, drawn_b_id, "the wired flow must route the receipt to the provider consensus draws");
            let b_key = env.bonds.iter().position(|o| palw_provider_id(o) == b_id).unwrap();
            let preimage = crate::match_key::decode_hex(&receipt_preimage_hex).unwrap();
            assert!(kaspa_consensus_core::palw::palw_receipt_embeds_a_commit(&preimage, &commitment));

            // (4) B signs; the bridge assembles, journals, and the REAL verifier accepts.
            let sig = mldsa::sign(&env.keys[b_key].signing_key, &preimage, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x44; 32]).unwrap();
            let produced = s
                .pcpb_partner_receipt(
                    &a_commit_hex,
                    &record.a_bond,
                    &bytes_hex_local(env.keys[b_key].verification_key.as_ref()),
                    &receipt_preimage_hex,
                    &bytes_hex_local(sig.as_ref()),
                    &env.complete,
                    40,
                )
                .unwrap();
            assert_eq!(produced.dispatch_kind, PALW_DISPATCH_KIND_SELF_SERIAL);
            assert_eq!(produced.a_commit_epoch, ANCHOR_EPOCH, "the leaf declares the epoch the CHAIN reported");
            let witness = produced.witness().unwrap();
            let resolved = kaspa_consensus_core::palw::PalwSnapshotCommitment {
                snapshot_root: crate::chain::parse_hash64(&produced.provider_snapshot_root_hex).unwrap(),
                assignment_root: crate::chain::parse_hash64(&produced.assignment_proof_root_hex).unwrap(),
                total_bond: 1000,
                provider_count: 4,
            };
            assert!(
                palw_dispatch_evidence_valid(&witness.dispatch, &resolved, &env.draw_seed, &facts_of(&produced), &TxscriptVerifier),
                "the wired path must emit evidence the production verifier accepts"
            );
            // The receipt post is idempotent (terminal state, no re-roll).
            let seq_after = s.seq();
            let again = s.pcpb_partner_receipt(&a_commit_hex, &record.a_bond, "00", "00", "00", &env.complete, 41).unwrap();
            assert_eq!(again, produced, "a second receipt post returns the SAME witness without re-verifying");
            assert_eq!(s.seq(), seq_after, "...and journals nothing new");

            // (5) Restart: journal replay restores the flow (Ready) and the witness byte-for-byte.
            drop(s);
            let mut s = BridgeState::open(&d, 120_000, 110).unwrap();
            let step = s.drive_pcpb_self_flow(&a_commit_hex, &record.a_bond, &env.complete, 50).unwrap();
            assert_eq!(
                step,
                PcpbSelfStepWire::Ready { a_commit_epoch: ANCHOR_EPOCH, leaf_challenge_hex: produced.leaf_challenge_hex.clone() }
            );
            assert_eq!(s.pcpb_witness(&produced.leaf_challenge_hex), Some(&produced));

            // The leaf challenge is the CONSENSUS derivation at the anchor epoch — what clause 11
            // will re-derive — not the bridge-domain Seam-1 value.
            let expected = kaspa_consensus_core::palw::palw_job_challenge(
                110,
                ANCHOR_EPOCH,
                &env.beacon_seed,
                &th(0xE1),
                &th(0xE2),
                &th(0xE3),
                1,
            );
            assert_eq!(produced.leaf_challenge_hex, hash64_hex(&expected));
        }

        /// External branch: the witness is produced FROM the Seam-1 lease (same triple, same anchor
        /// epoch), refuses a node whose `R_anchor` disagrees with the lease, is idempotent, and
        /// validates under the real verifier.
        #[test]
        fn pcpb_external_witness_is_lease_bound_and_idempotent() {
            let env = pcpb_env(&hash64_hex(&th(0xAA))); // a_commit unused on this branch
            let d = dir("pcpb-external");
            let mut s = BridgeState::open(&d, 120_000, 110).unwrap();
            register(&mut s, &env);
            let requester_bond = crate::chain::format_outpoint(&env.bonds[0]);

            let lease = s.lease_challenge(&requester_bond, &[1, 2, 3], 64, b"qi35-serve", 1, &env.complete, 5).unwrap();
            assert_eq!(lease.beacon_epoch, ANCHOR_EPOCH, "the lease anchors at the buried sample epoch");

            let produced = s.produce_pcpb_external_witness(&lease.job_challenge_hex, &requester_bond, &env.complete, 6).unwrap();
            assert_eq!(produced.dispatch_kind, PALW_DISPATCH_KIND_BEACON_ASSIGNED);
            assert_eq!(produced.a_commit_hex, hash64_hex(&Hash64::default()), "external carries the anchor sentinels");
            assert_eq!(produced.issued_epoch, ANCHOR_EPOCH);
            assert_eq!(produced.lease_job_challenge_hex.as_deref(), Some(lease.job_challenge_hex.as_str()));
            // The leaf challenge EQUALS the lease challenge: D3-b promoted Seam 1's derivation into
            // consensus byte-for-byte (same domain string, same preimage layout), so the field the
            // leaf commits is the very challenge the lease issued. This equality is load-bearing —
            // if it breaks, issued leases stop resolving on-chain.
            assert_eq!(produced.leaf_challenge_hex, lease.job_challenge_hex, "lease challenge IS the leaf challenge (byte parity)");
            let witness = produced.witness().unwrap();
            let resolved = kaspa_consensus_core::palw::PalwSnapshotCommitment {
                snapshot_root: crate::chain::parse_hash64(&produced.provider_snapshot_root_hex).unwrap(),
                assignment_root: crate::chain::parse_hash64(&produced.assignment_proof_root_hex).unwrap(),
                total_bond: 1000,
                provider_count: 4,
            };
            assert!(
                palw_dispatch_evidence_valid(&witness.dispatch, &resolved, &env.draw_seed, &facts_of(&produced), &TxscriptVerifier),
                "the wired external path must emit evidence the production verifier accepts"
            );
            // The witness triple IS the lease triple (clause 11's preimage).
            assert_eq!(hash64_hex(&witness.scheduler_job_id), lease.scheduler_job_id_hex);
            assert_eq!(hash64_hex(&witness.requester_credential), lease.requester_credential_hex);
            assert_eq!(hash64_hex(&witness.request_commitment), lease.request_commitment_hex);

            // Idempotent: same lease → same record, no new journal line.
            let seq = s.seq();
            assert_eq!(
                s.produce_pcpb_external_witness(&lease.job_challenge_hex, &requester_bond, &env.complete, 7).unwrap(),
                produced
            );
            assert_eq!(s.seq(), seq);

            // Another provider cannot claim the lease's witness route.
            let foreign = crate::chain::format_outpoint(&env.bonds[1]);
            assert!(
                s.produce_pcpb_external_witness(&lease.job_challenge_hex, &foreign, &env.complete, 8)
                    .unwrap_err()
                    .contains("different requester"),
                "the lease's requester binding must hold on the witness route"
            );

            // A node whose R_anchor disagrees with the lease's seed is refused — divergent beacon
            // histories must not silently produce evidence one side will reject.
            let d2 = dir("pcpb-external-divergent");
            let mut s2 = BridgeState::open(&d2, 120_000, 110).unwrap();
            register(&mut s2, &env);
            let lease2 = s2.lease_challenge(&requester_bond, &[1, 2, 3], 64, b"qi35-serve", 1, &env.complete, 5).unwrap();
            let divergent_seed = th(0x99);
            let divergent_env = pcpb_env_with_seed_mismatch(&env, divergent_seed);
            assert!(
                s2.produce_pcpb_external_witness(&lease2.job_challenge_hex, &requester_bond, &divergent_env, 6)
                    .unwrap_err()
                    .contains("differs from the lease"),
                "an R_anchor mismatch must refuse production"
            );

            // Restart: the produced witness is still servable.
            drop(s);
            let s = BridgeState::open(&d, 120_000, 110).unwrap();
            assert_eq!(s.pcpb_witness(&produced.leaf_challenge_hex), Some(&produced));
        }

        /// The same pinned world with the anchor seed replaced — models a node on a different
        /// beacon history than the one the lease was issued under.
        fn pcpb_env_with_seed_mismatch(env: &PcpbEnv, seed: Hash64) -> PinnedChainFacts {
            let entries = env
                .bonds
                .iter()
                .enumerate()
                .map(|(i, bond)| PinnedPcpbEntry {
                    provider_id_hex: hash64_hex(&palw_provider_id(bond)),
                    ml_dsa_pk_hash_hex: hash64_hex(&palw_provider_pk_hash(env.keys[i].verification_key.as_ref())),
                    bond_sompi: 250,
                    reward_script_commitment_hex: hash64_hex(&th(0xD0 + i as u8)),
                })
                .collect();
            let beacon = BeaconFacts {
                epoch: ANCHOR_EPOCH,
                seed_hex: hash64_hex(&env.beacon_seed),
                anchor_hash_hex: "cd".repeat(64),
                anchor_daa_score: ANCHOR_EPOCH * 100,
                observed_daa_score: ANCHOR_EPOCH * 100 + 300,
                current_epoch: ANCHOR_EPOCH + 1,
            };
            let mut file =
                PinnedFactsFile { beacon, bonds: BTreeMap::new(), pcpb_anchors: BTreeMap::new(), pcpb_acommits: BTreeMap::new() };
            file.pcpb_anchors.insert(
                ANCHOR_EPOCH,
                PinnedPcpbAnchor {
                    snapshot_epoch: ANCHOR_EPOCH - 2,
                    draw_epoch: ANCHOR_EPOCH + 2,
                    entries,
                    anchor_seed_hex: hash64_hex(&seed),
                    draw_seed_hex: Some(hash64_hex(&env.draw_seed)),
                },
            );
            PinnedChainFacts::from_facts(file)
        }

        /// `palw_pcpb_derive_b` is deterministic in `a_commit`, so the fixture search space is
        /// small; assert the helper agrees with the wired flow's chosen partner (drift here would
        /// mean the bridge routes the receipt to a provider consensus did not draw).
        #[test]
        fn pcpb_partner_agrees_with_the_consensus_draw() {
            let commitment = crate::pcpb::a_commit(b"job-descriptor", &th(0x77), &[0x33; 32]);
            let a_commit_hex = hash64_hex(&commitment);
            let env = pcpb_env(&a_commit_hex);
            let d = dir("pcpb-draw-agreement");
            let mut s = BridgeState::open(&d, 120_000, 110).unwrap();
            register(&mut s, &env);
            let record = PcpbSelfFlowRecordV1 {
                a_commit_hex: a_commit_hex.clone(),
                a_bond: crate::chain::format_outpoint(&env.bonds[0]),
                scheduler_job_id_hex: hash64_hex(&th(0xE1)),
                requester_credential_hex: hash64_hex(&th(0xE2)),
                request_commitment_hex: hash64_hex(&th(0xE3)),
                shape_id: 1,
                receipt_tail_hex: bytes_hex_local(b"self-tail"),
            };
            let step = s.open_pcpb_self_flow(&record, &env.complete, 10).unwrap();
            let PcpbSelfStepWire::AwaitPartnerReceipt { partner_bond, .. } = step else {
                panic!("expected AwaitPartnerReceipt, got {step:?}");
            };
            // Recompute B exactly as consensus will.
            let consensus_entries: Vec<_> = env
                .bonds
                .iter()
                .enumerate()
                .map(|(i, bond)| kaspa_consensus_core::palw::PalwProviderSnapshotEntry {
                    provider_id: palw_provider_id(bond),
                    ml_dsa_pk_hash: palw_provider_pk_hash(env.keys[i].verification_key.as_ref()),
                    bond_sompi: 250,
                    reward_script_commitment: th(0xD0 + i as u8),
                })
                .collect();
            let ws = palw_build_snapshot_witnesses(&consensus_entries);
            let slot = ws.select(&palw_pcpb_derive_b(&env.draw_seed, &commitment)).unwrap();
            let expected = ws.slots[slot].entry.provider_id;
            assert_eq!(palw_provider_id(&crate::chain::parse_outpoint(&partner_bond).unwrap()), expected);
        }
    }
}
