//! kaspa-pq remote signer / HSM daemon core (ADR-0015, audit H-04).
//!
//! Transport-agnostic signing engine: holds the ML-DSA-87 validator key(s), enforces the signing
//! policy ([`SignerPolicy`]), keeps an equivocation guard ([`SignedEpochStore`]) and a tamper-evident
//! audit log, and turns a [`SignerRequest`] into a [`SignerResponse`]. The binary ([`crate`]'s
//! `main.rs`) wraps this in a Unix-domain-socket frame loop; tests drive [`SignerState::handle_request`]
//! directly. The validator NODE never sees the key — it sends a digest + purpose and gets a signature
//! back (or a policy refusal), so a node compromise cannot directly exfiltrate the key or equivocate.
//!
//! Reused, audited building blocks (no new crypto/protocol invented here):
//! * the protocol types + [`compute_signer_audit_chain_entry`] + [`signature_fingerprint`] from
//!   `kaspa_consensus_core::dns_finality`;
//! * [`ValidatorKey`] (signing) + [`SignedEpochStore`] (crash-consistent, fsync'd equivocation log)
//!   from `kaspa_pq_validator_core`;
//! * the pure equivocation decision [`check_signed_epoch_record`].

use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use kaspa_consensus_core::{
    dns_finality::{
        ATTESTATION_MLDSA87_CONTEXT, AUDIT_CHECKPOINT_MLDSA87_CONTEXT, HostId, SignedEpochCheckOutcome, SignedEpochRecord,
        SignerAuditCheckpoint, SignerAuditRecord, SignerError, SignerMessageDigest, SignerMetadata, SignerOutcome, SignerPolicy,
        SignerRequest, SignerResponse, SigningPurpose, TAKEOVER_TOKEN_CONTEXT, UNBOND_REQUEST_CONTEXT,
        compute_signer_audit_chain_entry, signature_fingerprint,
    },
    tx::TransactionOutpoint,
};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::{SignedEpochStore, ValidatorKey};

/// An append-only, hash-chained, fsync'd audit log (ADR-0015 §"Audit log"). Each record extends the
/// chain via [`compute_signer_audit_chain_entry`]; on load the file is replayed to recompute the
/// chain head, so an inserted/deleted/modified record is detectable (its successors' hashes shift).
/// Records are stored as 4-byte-big-endian-length-prefixed Borsh frames.
struct AuditLog {
    path: PathBuf,
    chain_hash: Hash64,
    /// Number of records currently in the log (recovered on load,
    /// incremented on append). Stamped into each checkpoint's
    /// `record_index` so a verifier can locate the exact prefix.
    record_count: u64,
}

impl AuditLog {
    fn load_or_new(path: PathBuf) -> io::Result<Self> {
        let mut chain_hash = Hash64::default();
        let mut record_count: u64 = 0;
        if path.exists() {
            let bytes = fs::read(&path)?;
            let mut cur = &bytes[..];
            // Byte offset of the end of the last FULLY-written frame.
            let mut good_len: usize = 0;
            while cur.len() >= 4 {
                let len = u32::from_be_bytes(cur[0..4].try_into().expect("4 bytes")) as usize;
                if cur.len() < 4 + len {
                    break; // a torn trailing frame (crash mid-append).
                }
                let rec: SignerAuditRecord = borsh::from_slice(&cur[4..4 + len])?;
                chain_hash = compute_signer_audit_chain_entry(chain_hash, &rec);
                record_count += 1;
                cur = &cur[4 + len..];
                good_len += 4 + len;
            }
            // If a torn/partial trailing frame remains, TRUNCATE the log to the last good record and
            // fsync. Otherwise the torn bytes would shadow every record appended after them on the
            // next load (the append is `O_APPEND`, so new records land *after* the torn frame and are
            // never replayed) — an audit-trail gap. The torn frame was a never-completed record, so
            // dropping it loses nothing durable.
            if good_len < bytes.len() {
                let f = OpenOptions::new().write(true).open(&path)?;
                f.set_len(good_len as u64)?;
                f.sync_all()?;
                log::warn!(
                    "[signer] repaired audit log {}: truncated {} torn trailing byte(s) to the last good record",
                    path.display(),
                    bytes.len() - good_len
                );
            }
        }
        Ok(Self { path, chain_hash, record_count })
    }

    /// Append `rec`, advance + return the new chain head, and fsync (fail-closed: the caller treats
    /// an audit-write error as a refusal, never silently signing without an audit trail).
    fn append(&mut self, rec: &SignerAuditRecord) -> io::Result<Hash64> {
        let next = compute_signer_audit_chain_entry(self.chain_hash, rec);
        let body = borsh::to_vec(rec)?;
        let mut frame = (body.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&body);
        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600); // owner-only audit log
        }
        let mut f = opts.open(&self.path)?;
        f.write_all(&frame)?;
        f.sync_all()?;
        self.chain_hash = next;
        self.record_count += 1;
        Ok(self.chain_hash)
    }

    fn chain_head(&self) -> Hash64 {
        self.chain_hash
    }

    fn record_count(&self) -> u64 {
        self.record_count
    }

    /// Audit M-04: re-replay the on-disk log, capturing the chain head AFTER exactly `k` records for
    /// every `k` in `wanted`. Returns the captured heads keyed by record count plus the total record
    /// count, so the checkpoint verifier can confirm each signed `chain_head` matches the recomputed
    /// prefix (a tampered/truncated log shifts the head and is caught). Walks only good frames — a
    /// torn trailing frame is ignored exactly as `load_or_new` would truncate it.
    fn replay_heads_at(path: &Path, wanted: &BTreeSet<u64>) -> io::Result<(HashMap<u64, Hash64>, u64)> {
        let mut heads = HashMap::new();
        let mut count: u64 = 0;
        let mut chain = Hash64::default();
        if !path.exists() {
            return Ok((heads, 0));
        }
        let bytes = fs::read(path)?;
        let mut cur = &bytes[..];
        while cur.len() >= 4 {
            let len = u32::from_be_bytes(cur[0..4].try_into().expect("4 bytes")) as usize;
            if cur.len() < 4 + len {
                break;
            }
            let rec: SignerAuditRecord = borsh::from_slice(&cur[4..4 + len])?;
            chain = compute_signer_audit_chain_entry(chain, &rec);
            count += 1;
            if wanted.contains(&count) {
                heads.insert(count, chain);
            }
            cur = &cur[4 + len..];
        }
        Ok((heads, count))
    }
}

/// Audit M-04: read the framed [`SignerAuditCheckpoint`] records from `path` (4-byte big-endian
/// length prefix + Borsh body), tolerating a torn trailing frame exactly as the audit log does.
fn read_audit_checkpoints(path: &Path) -> io::Result<Vec<SignerAuditCheckpoint>> {
    let mut out = Vec::new();
    if !path.exists() {
        return Ok(out);
    }
    let bytes = fs::read(path)?;
    let mut cur = &bytes[..];
    while cur.len() >= 4 {
        let len = u32::from_be_bytes(cur[0..4].try_into().expect("4 bytes")) as usize;
        if cur.len() < 4 + len {
            break; // torn trailing frame
        }
        out.push(borsh::from_slice(&cur[4..4 + len])?);
        cur = &cur[4 + len..];
    }
    Ok(out)
}

/// Audit M-04: summary of the startup checkpoint verification.
struct CheckpointReport {
    total: usize,
    anomalies: usize,
    latest_index: u64,
    latest_head: Hash64,
}

/// Audit M-04: verify every signed checkpoint against the recomputed audit chain. A checkpoint is an
/// anomaly if (a) its ML-DSA-87 signature does not verify against the held key for its `validator_id`
/// (forged/corrupt, or a key the signer no longer holds), or (b) the chain head recomputed from
/// `audit.log` at its `record_index` differs from the signed `chain_head` — i.e. the log was
/// rewritten or truncated below the checkpoint. Detection only; the caller logs and never refuses to
/// start.
fn verify_audit_checkpoints(
    audit_log_path: &Path,
    checkpoint_path: &Path,
    keys: &HashMap<Hash64, ValidatorKey>,
) -> io::Result<CheckpointReport> {
    let ckpts = read_audit_checkpoints(checkpoint_path)?;
    if ckpts.is_empty() {
        return Ok(CheckpointReport { total: 0, anomalies: 0, latest_index: 0, latest_head: Hash64::default() });
    }
    let wanted: BTreeSet<u64> = ckpts.iter().map(|c| c.record_index).collect();
    let (heads_at, total_records) = AuditLog::replay_heads_at(audit_log_path, &wanted)?;

    let mut anomalies = 0usize;
    for c in &ckpts {
        // (a) signature: must verify under the held key for this validator_id.
        let sig_ok = keys
            .get(&c.validator_id)
            .is_some_and(|k| k.verify_with_context(c.chain_head.as_byte_slice(), &c.signature, AUDIT_CHECKPOINT_MLDSA87_CONTEXT));
        if !sig_ok {
            anomalies += 1;
            log::error!(
                "[signer] audit checkpoint #{} (M-04): SIGNATURE INVALID for validator {} — forged/corrupt checkpoint or unknown key",
                c.record_index,
                c.validator_id
            );
            continue;
        }
        // (b) head consistency: the chain head recomputed from the current log at record_index must
        //     equal the signed head.
        match heads_at.get(&c.record_index) {
            Some(h) if *h == c.chain_head => {}
            Some(_) => {
                anomalies += 1;
                log::error!(
                    "[signer] audit checkpoint #{} (M-04): chain head MISMATCH — audit.log was rewritten at or before record {}",
                    c.record_index,
                    c.record_index
                );
            }
            None => {
                anomalies += 1;
                log::error!(
                    "[signer] audit checkpoint #{} (M-04): audit.log holds only {} record(s) — records were DELETED below this checkpoint",
                    c.record_index,
                    total_records
                );
            }
        }
    }
    let latest = ckpts.iter().max_by_key(|c| c.record_index).expect("non-empty");
    Ok(CheckpointReport { total: ckpts.len(), anomalies, latest_index: latest.record_index, latest_head: latest.chain_head })
}

/// Audit M-04: take a signed audit-log checkpoint at least this often (every N appended records).
/// Small enough that the unprotected tail of the log is short; large enough that the extra ML-DSA-87
/// signature is negligible against the signing the signer is already doing.
const AUDIT_CHECKPOINT_INTERVAL: u64 = 32;

/// The signing engine: keys, policy, per-validator equivocation guard, and the audit log.
pub struct SignerState {
    keys: HashMap<Hash64, ValidatorKey>,
    policy: SignerPolicy,
    state_dir: PathBuf,
    /// Per-`validator_id` equivocation log. The signer guards on `(validator_id, epoch)` — a
    /// validator signing two different targets for one epoch is slashable regardless of bond — so a
    /// fixed default `bond_outpoint` keys the reused [`SignedEpochStore`] purely by epoch.
    epoch_stores: HashMap<Hash64, SignedEpochStore>,
    audit: AuditLog,
    /// Audit M-04: append-only log of [`SignerAuditCheckpoint`]s anchoring the audit chain head with
    /// an ML-DSA-87 signature. Lives beside `audit.log`; meant to be exported off-box.
    checkpoint_path: PathBuf,
    /// Records appended since the last checkpoint (triggers the next at [`AUDIT_CHECKPOINT_INTERVAL`]).
    appends_since_checkpoint: u64,
    server_identity: HostId,
    /// Optional policy hook: purposes this signer refuses to sign (e.g. a validator-only signer can
    /// deny `Transaction` so it only ever produces attestation/unbond/takeover signatures). Empty by
    /// default — no behavior change unless the operator opts in.
    denied_purposes: Vec<SigningPurpose>,
}

impl SignerState {
    /// Build the engine over `keys` (deduplicated by `validator_id`), persisting equivocation logs +
    /// the audit log under `state_dir` (created if absent). The audit chain head is recovered from
    /// any existing log.
    pub fn new(keys: Vec<ValidatorKey>, policy: SignerPolicy, state_dir: PathBuf, server_identity: HostId) -> Result<Self, String> {
        fs::create_dir_all(&state_dir).map_err(|e| format!("cannot create signer state dir {}: {e}", state_dir.display()))?;
        // The state dir holds the equivocation logs and the audit log — owner-only (0700).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("cannot chmod 700 signer state dir {}: {e}", state_dir.display()))?;
        }
        let audit = AuditLog::load_or_new(state_dir.join("audit.log")).map_err(|e| format!("cannot open signer audit log: {e}"))?;
        let keys: HashMap<Hash64, ValidatorKey> = keys.into_iter().map(|k| (k.validator_id, k)).collect();
        let checkpoint_path = state_dir.join("audit.checkpoints");
        // Audit M-04: at startup, verify any existing signed checkpoints against the recomputed audit
        // chain. Detection only — anomalies are logged loudly, never a refusal-to-start (a tampered
        // log must not be able to DoS the signer; the operator acts on the alert + the off-box copy).
        let audit_log_path = state_dir.join("audit.log");
        match verify_audit_checkpoints(&audit_log_path, &checkpoint_path, &keys) {
            Ok(report) => {
                if report.total == 0 {
                    log::info!(
                        "[signer] no audit checkpoints yet (M-04); first will be signed after {AUDIT_CHECKPOINT_INTERVAL} records"
                    );
                } else if report.anomalies == 0 {
                    log::info!(
                        "[signer] verified {} audit checkpoint(s) (M-04): latest #{} head {} — 0 anomalies",
                        report.total,
                        report.latest_index,
                        report.latest_head
                    );
                } else {
                    log::error!(
                        "[signer] AUDIT TAMPER ALERT (M-04): {} of {} checkpoint(s) FAILED verification — audit.log may have been rewritten. Compare against the off-box checkpoint copy.",
                        report.anomalies,
                        report.total
                    );
                }
            }
            Err(e) => log::warn!("[signer] could not verify audit checkpoints (M-04): {e}"),
        }
        Ok(Self {
            keys,
            policy,
            state_dir,
            epoch_stores: HashMap::new(),
            audit,
            checkpoint_path,
            appends_since_checkpoint: 0,
            server_identity,
            denied_purposes: Vec::new(),
        })
    }

    /// Set the optional purpose denylist (see [`SignerState::denied_purposes`]). A request whose
    /// purpose is denied is refused with a policy violation before any signing.
    pub fn set_denied_purposes(&mut self, purposes: Vec<SigningPurpose>) {
        self.denied_purposes = purposes;
    }

    /// The signer's own host identity (echoed in the handshake ack).
    pub fn server_identity(&self) -> HostId {
        self.server_identity
    }

    /// The current audit-log chain head (for status / external verification).
    pub fn audit_chain_head(&self) -> Hash64 {
        self.audit.chain_head()
    }

    /// `validator_id`s this signer can sign for.
    pub fn known_validators(&self) -> Vec<Hash64> {
        self.keys.keys().copied().collect()
    }

    fn epoch_store(&mut self, vid: Hash64) -> Result<&mut SignedEpochStore, String> {
        if !self.epoch_stores.contains_key(&vid) {
            let path = self.state_dir.join(format!("{vid}.epochs.json"));
            let store = SignedEpochStore::load_or_empty(path, vid, TransactionOutpoint::default())?;
            self.epoch_stores.insert(vid, store);
        }
        Ok(self.epoch_stores.get_mut(&vid).expect("just inserted"))
    }

    /// Handle one request: enforce policy, sign (or refuse), append an audit record, and return the
    /// response. `now_unix_secs` is injected (the binary passes wall-clock; tests pass a fixed value)
    /// so the engine itself stays deterministic. An audit-write failure is itself a refusal.
    pub fn handle_request(&mut self, req: &SignerRequest, client: HostId, now_unix_secs: u64) -> SignerResponse {
        let (result, outcome, fingerprint) = match self.try_sign(req) {
            Ok(sig) => {
                let fp = signature_fingerprint(&sig);
                (Ok(sig), SignerOutcome::Signed, fp)
            }
            Err(e) => (Err(e.clone()), SignerOutcome::Refused(e), Hash64::default()),
        };
        let record = SignerAuditRecord {
            timestamp_unix_secs: now_unix_secs,
            client_identity: client,
            request_id: req.request_id,
            validator_id: req.validator_id,
            purpose: req.purpose,
            metadata: req.metadata.clone(),
            message_digest: req.message_digest.clone(),
            signature_fingerprint: fingerprint,
            outcome,
        };
        if let Err(e) = self.audit.append(&record) {
            // Fail-closed: never return a signature we could not durably audit.
            return SignerResponse {
                request_id: req.request_id,
                result: Err(SignerError::InternalError(format!("audit write failed: {e}"))),
            };
        }
        // Audit M-04: anchor the audit chain head with a signed checkpoint every
        // AUDIT_CHECKPOINT_INTERVAL records. Best-effort — a checkpoint-write failure is logged but
        // does NOT fail the request (the fail-closed audit append above is the durability gate; the
        // checkpoint is supplementary tamper-evidence, and a full disk must not be able to DoS
        // signing through this path).
        self.appends_since_checkpoint += 1;
        if self.appends_since_checkpoint >= AUDIT_CHECKPOINT_INTERVAL {
            self.appends_since_checkpoint = 0;
            if let Err(e) = self.write_audit_checkpoint(now_unix_secs) {
                log::warn!("[signer] audit checkpoint write failed (M-04, non-fatal): {e}");
            }
        }
        SignerResponse { request_id: req.request_id, result }
    }

    /// Audit M-04: sign the current audit-log chain head with a held validator ML-DSA-87 key (under
    /// [`AUDIT_CHECKPOINT_MLDSA87_CONTEXT`]) and append a [`SignerAuditCheckpoint`] to
    /// `audit.checkpoints`. The signing key is chosen deterministically (smallest `validator_id`) so
    /// reloads and tests are reproducible. The chain key is public, so this signature — which an
    /// on-host attacker cannot forge — is what makes the audit log tamper-EVIDENT once a copy of the
    /// checkpoint file is held off-box.
    fn write_audit_checkpoint(&mut self, now_unix_secs: u64) -> io::Result<()> {
        // Deterministic signer: the smallest known validator_id.
        let Some(vid) = self.keys.keys().copied().min() else {
            return Ok(()); // no keys loaded — nothing to anchor with (cannot happen for a live signer)
        };
        let key = self.keys.get(&vid).expect("validator_id from this map");
        let head = self.audit.chain_head();
        let record_index = self.audit.record_count();
        let signature = key.sign_with_context(head.as_byte_slice(), AUDIT_CHECKPOINT_MLDSA87_CONTEXT).to_vec();
        let ckpt =
            SignerAuditCheckpoint { record_index, timestamp_unix_secs: now_unix_secs, validator_id: vid, chain_head: head, signature };

        let body = borsh::to_vec(&ckpt)?;
        let mut frame = (body.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&body);
        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&self.checkpoint_path)?;
        f.write_all(&frame)?;
        f.sync_all()?;
        log::info!(
            "[signer] audit checkpoint #{record_index} taken (M-04): head {head} signed by validator {vid} @ {now_unix_secs}s — export {} off-box for tamper-evidence",
            self.checkpoint_path.display()
        );
        Ok(())
    }

    /// Policy-gated signing. Returns the raw ML-DSA-87 signature bytes or a structured error.
    fn try_sign(&mut self, req: &SignerRequest) -> Result<Vec<u8>, SignerError> {
        // (1) Wellformedness: the purpose tag MUST match the typed digest variant (audit H-03).
        if !req.purpose_matches_digest() {
            return Err(SignerError::PolicyViolation("purpose tag does not match message_digest variant".into()));
        }
        // (1-C02) Bind the ML-DSA-87 signing CONTEXT to the purpose (audit C-02, Critical).
        // The context is the cryptographic domain separator that makes a signature
        // verify as a SPECIFIC operation. Without this binding a caller could submit
        // purpose=Unbond with the Unbond(attestation_digest) variant AND
        // context=ATTESTATION_MLDSA87_CONTEXT: purpose_matches_digest passes (Unbond
        // tag == Unbond digest), the Attestation-only equivocation guard below is
        // SKIPPED (purpose != Attestation), and the produced signature nonetheless
        // verifies as a canonical attestation — a double-sign from an isolated key
        // that defeats the whole point of the strict signer. So the three overlay
        // contexts are RESERVED to their matching purpose, and a Transaction may not
        // borrow any of them (it carries its own tx-domain context).
        const OVERLAY_CONTEXTS: [&[u8]; 3] = [ATTESTATION_MLDSA87_CONTEXT, UNBOND_REQUEST_CONTEXT, TAKEOVER_TOKEN_CONTEXT];
        let required_ctx: Option<&[u8]> = match req.purpose {
            SigningPurpose::Attestation => Some(ATTESTATION_MLDSA87_CONTEXT),
            SigningPurpose::Unbond => Some(UNBOND_REQUEST_CONTEXT),
            SigningPurpose::TakeoverToken => Some(TAKEOVER_TOKEN_CONTEXT),
            SigningPurpose::Transaction => None,
        };
        match required_ctx {
            Some(ctx) if req.context.as_slice() != ctx => {
                return Err(SignerError::PolicyViolation(format!(
                    "signing context does not match purpose {:?} (audit C-02)",
                    req.purpose
                )));
            }
            None if OVERLAY_CONTEXTS.iter().any(|c| *c == req.context.as_slice()) => {
                return Err(SignerError::PolicyViolation(
                    "Transaction purpose may not borrow an overlay signing context (audit C-02)".into(),
                ));
            }
            _ => {}
        }
        // (1a) Optional purpose denylist (e.g. a validator-only signer that refuses Transaction).
        if self.denied_purposes.contains(&req.purpose) {
            return Err(SignerError::PolicyViolation(format!("signing purpose {:?} is denied by policy", req.purpose)));
        }
        // (1b) The ML-DSA-87 signing context is bounded to 255 bytes (FIPS 204). Reject an over-long
        //      context in-band here rather than letting it reach the `assert!` in
        //      `ValidatorKey::sign_with_context`, which would panic — and, under the shared state
        //      mutex, poison it and wedge every subsequent request (remote DoS).
        if req.context.len() > 255 {
            return Err(SignerError::PolicyViolation(format!("signing context exceeds 255 bytes (got {})", req.context.len())));
        }
        // (2) The signer must hold this validator's key.
        if !self.keys.contains_key(&req.validator_id) {
            return Err(SignerError::KeyNotFound);
        }

        // (3) Equivocation guard (ADR-0011) — only for attestations, only when not Permissive.
        //     Strict refuses a conflicting (validator, epoch) target; AuditOnly logs but allows.
        let mut record_after_sign: Option<SignedEpochRecord> = None;
        if matches!(req.purpose, SigningPurpose::Attestation) && !matches!(self.policy, SignerPolicy::Permissive) {
            let SignerMetadata::Attestation { epoch, target_hash, target_daa_score } = req.metadata else {
                return Err(SignerError::PolicyViolation("attestation request missing Attestation metadata (epoch/target)".into()));
            };
            let candidate = SignedEpochRecord { epoch, target_hash, target_daa_score, signature_fingerprint: Hash64::default() };
            let policy = self.policy;
            let store = self.epoch_store(req.validator_id).map_err(SignerError::InternalError)?;
            match store.check(&candidate) {
                SignedEpochCheckOutcome::Block => {
                    let msg =
                        format!("equivocation: epoch {epoch} already signed a different target (validator {})", req.validator_id);
                    if matches!(policy, SignerPolicy::Strict) {
                        return Err(SignerError::PolicyViolation(msg));
                    }
                    // AuditOnly: surface the conflict but proceed (migration mode).
                    log::warn!("[signer] AuditOnly: {msg} — signing anyway");
                }
                // First time for this epoch: persist the record AFTER a successful sign.
                SignedEpochCheckOutcome::Allow => record_after_sign = Some(candidate),
                // Same target again: already recorded; safe to re-sign without re-recording.
                SignedEpochCheckOutcome::AllowRebroadcast => {}
            }
        }

        // (4) Sign the typed digest with the caller-provided ML-DSA-87 context.
        let key = self.keys.get(&req.validator_id).expect("checked above");
        let digest: Vec<u8> = match &req.message_digest {
            SignerMessageDigest::Transaction(h) => h.as_bytes().to_vec(),
            SignerMessageDigest::Attestation(h) | SignerMessageDigest::Unbond(h) | SignerMessageDigest::TakeoverToken(h) => {
                h.as_bytes().to_vec()
            }
        };
        let sig = key.sign_with_context(&digest, &req.context);

        // (5) Persist the equivocation record only on a brand-new (validator, epoch) attestation,
        //     stamping the real signature fingerprint (fail-closed if the durable write fails).
        if let Some(mut rec) = record_after_sign {
            rec.signature_fingerprint = signature_fingerprint(&sig);
            let store = self.epoch_store(req.validator_id).map_err(SignerError::InternalError)?;
            store.record_and_flush(rec).map_err(SignerError::InternalError)?;
        }
        Ok(sig.to_vec())
    }
}

/// Unix-domain-socket transport for the signer (ADR-0015). The frame format is a 4-byte big-endian
/// length prefix followed by a Borsh body; a connection is a [`SignerHello`]/[`SignerHelloAck`]
/// handshake then a stream of [`SignerRequest`]/[`SignerResponse`] frames. Lives in the lib (not the
/// binary) so it is exercised by an end-to-end integration test, not just compiled.
#[cfg(unix)]
pub mod transport {
    use super::SignerState;
    use kaspa_consensus_core::dns_finality::{
        HostId, SIGNER_PROTOCOL_VERSION, SignerError, SignerHello, SignerHelloAck, SignerRequest, SignerResponse,
    };
    use std::{
        io::{self, Read, Write},
        os::unix::net::UnixStream,
        path::Path,
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    /// A signer frame is one Borsh request/response/hello; a request carries at most a 64-byte digest
    /// + a short context, so 64 KiB is a generous DoS bound on a single frame.
    pub const MAX_FRAME: usize = 64 * 1024;

    pub fn read_frame(r: &mut impl Read) -> io::Result<Vec<u8>> {
        let mut len_buf = [0u8; 4];
        r.read_exact(&mut len_buf)?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_FRAME {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "signer frame exceeds MAX_FRAME"));
        }
        let mut buf = vec![0u8; len];
        r.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn write_frame(w: &mut impl Write, body: &[u8]) -> io::Result<()> {
        w.write_all(&(body.len() as u32).to_be_bytes())?;
        w.write_all(body)?;
        w.flush()
    }

    fn now_unix_secs() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }

    /// Peer-credential (uid) of a connected client. `None` if it cannot be determined (the caller
    /// then falls back to the socket file-mode boundary only). The mechanism is OS-specific:
    /// Linux/Android use `SO_PEERCRED` (`getpeereid` is not declared for Linux in `libc`); the BSDs
    /// and macOS use `getpeereid(2)`; other Unixes fall back to file-mode only.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn peer_uid(stream: &UnixStream) -> Option<u32> {
        use std::os::unix::io::AsRawFd;
        let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
        let mut len = core::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let rc = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                core::ptr::addr_of_mut!(cred).cast::<libc::c_void>(),
                &mut len,
            )
        };
        if rc == 0 { Some(cred.uid) } else { None }
    }

    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    fn peer_uid(stream: &UnixStream) -> Option<u32> {
        use std::os::unix::io::AsRawFd;
        let mut uid: libc::uid_t = 0;
        let mut gid: libc::gid_t = 0;
        let rc = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
        if rc == 0 { Some(uid) } else { None }
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )))]
    fn peer_uid(_stream: &UnixStream) -> Option<u32> {
        // No portable peer-credential API on this platform: rely on the 0700 socket perms.
        None
    }

    /// Serve one client connection: handshake (version-check + ack), then a request/response loop
    /// until the peer closes. Each request is signed (or refused) through the shared [`SignerState`].
    pub fn serve_connection(mut stream: UnixStream, state: &Arc<Mutex<SignerState>>, server_identity: HostId, allowed_uids: &[u32]) {
        // Peer-credential check: only the signer's own uid (or, if configured, an explicit allowlist)
        // may obtain signatures. The socket's 0700 perms already enforce this, but getpeereid() is
        // defense-in-depth against a mis-set mode or a residual bind/chmod race — without it, file-mode
        // is the *only* gate (audit E/F7).
        if let Some(peer) = peer_uid(&stream) {
            let me = unsafe { libc::geteuid() };
            let allowed = if allowed_uids.is_empty() { peer == me } else { allowed_uids.contains(&peer) };
            if !allowed {
                log::warn!("[signer] rejecting connection from uid {peer} (signer uid {me}, allowlist {allowed_uids:?})");
                return;
            }
        }

        // Read timeout: each connection holds a dedicated OS thread, so a client that connects and
        // then never sends a frame would otherwise pin that thread forever. A per-read deadline reaps
        // such idle connections (local DoS hardening; the peer is already same-uid).
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));

        let hello: SignerHello = match read_frame(&mut stream).and_then(|b| borsh::from_slice(&b).map_err(io::Error::other)) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("[signer] handshake read failed: {e}");
                return;
            }
        };
        if hello.protocol_version != SIGNER_PROTOCOL_VERSION {
            log::warn!("[signer] protocol version mismatch: client {} vs signer {SIGNER_PROTOCOL_VERSION}", hello.protocol_version);
            let resp = SignerResponse { request_id: 0, result: Err(SignerError::ProtocolVersionMismatch) };
            let _ = write_frame(&mut stream, &borsh::to_vec(&resp).unwrap_or_default());
            return;
        }
        let client = hello.client_identity;
        let ack = SignerHelloAck { protocol_version: SIGNER_PROTOCOL_VERSION, capabilities: 0, server_identity };
        if write_frame(&mut stream, &borsh::to_vec(&ack).expect("ack borsh")).is_err() {
            return;
        }
        // Handshake done: clear the read deadline so a legitimate long-lived client (e.g. a validator
        // that idles between attestations) is not disconnected mid-session. The pre-handshake timeout
        // above already reaps connect-and-hold attempts.
        let _ = stream.set_read_timeout(None);

        loop {
            let body = match read_frame(&mut stream) {
                Ok(b) => b,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return, // peer closed
                Err(e) => {
                    log::warn!("[signer] request read failed: {e}");
                    return;
                }
            };
            let req: SignerRequest = match borsh::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("[signer] malformed request frame: {e}");
                    let resp =
                        SignerResponse { request_id: 0, result: Err(SignerError::InternalError(format!("malformed request: {e}"))) };
                    let _ = write_frame(&mut stream, &borsh::to_vec(&resp).unwrap_or_default());
                    return;
                }
            };
            // Recover from a poisoned mutex rather than re-panicking: a panic while handling an earlier
            // request must not permanently wedge the signer for all future connections.
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            let resp = guard.handle_request(&req, client, now_unix_secs());
            drop(guard);
            if write_frame(&mut stream, &borsh::to_vec(&resp).expect("response borsh")).is_err() {
                return;
            }
        }
    }

    /// A blocking client to a running signer daemon (the validator's view of the HSM). `connect`
    /// performs the version-checked handshake; `sign` sends one [`SignerRequest`] and awaits its
    /// [`SignerResponse`]. This is the production path a validator uses to route attestation/unbond
    /// signing through the daemon — the key never enters the validator process. The e2e test below
    /// drives the daemon through exactly this API.
    pub struct SignerClient {
        stream: UnixStream,
        /// The daemon's [`HostId`], learned from the handshake ack (for audit attribution / pinning).
        pub server_identity: HostId,
    }

    impl SignerClient {
        /// Connect to the daemon at `socket_path` and perform the handshake. Errors on a transport
        /// failure or a protocol-version mismatch (the daemon closes the connection in that case).
        pub fn connect(socket_path: impl AsRef<Path>, client_identity: HostId) -> io::Result<Self> {
            let mut stream = UnixStream::connect(socket_path)?;
            let hello = SignerHello { protocol_version: SIGNER_PROTOCOL_VERSION, capabilities: 0, client_identity };
            write_frame(&mut stream, &borsh::to_vec(&hello)?)?;
            let ack: SignerHelloAck = borsh::from_slice(&read_frame(&mut stream)?).map_err(io::Error::other)?;
            if ack.protocol_version != SIGNER_PROTOCOL_VERSION {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "signer protocol version mismatch"));
            }
            Ok(Self { stream, server_identity: ack.server_identity })
        }

        /// Send one sign request and await the daemon's response. A transport error is an `Err`; a
        /// policy/key refusal arrives in-band as `Ok(SignerResponse { result: Err(..), .. })`.
        pub fn sign(&mut self, req: &SignerRequest) -> io::Result<SignerResponse> {
            write_frame(&mut self.stream, &borsh::to_vec(req)?)?;
            borsh::from_slice(&read_frame(&mut self.stream)?).map_err(io::Error::other)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::dns_finality::{ATTESTATION_MLDSA87_CONTEXT, stake_attestation_message};
    use kaspa_hashes::Hash;
    use kaspa_pq_validator_core::VALIDATOR_SEED_LEN;

    // The tx-signing context lives in kaspa_txscript; the signer signs with whatever context it is
    // handed (it does not infer/validate it, per ADR-0015), so the test uses the literal value.
    const MLDSA87_TX_CONTEXT: &[u8] = b"kaspa-pq-v2/tx/mldsa87";

    fn tmp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        // Deterministic per-test dir (no Date/rand); cleaned at the start of each test.
        p.push(format!("kaspa-pq-signer-test-{tag}"));
        let _ = fs::remove_dir_all(&p);
        p
    }

    fn key(seed: u8) -> ValidatorKey {
        ValidatorKey::from_seed([seed; VALIDATOR_SEED_LEN])
    }

    fn att_request(req_id: u64, vid: Hash64, epoch: u64, target: Hash64, daa: u64) -> SignerRequest {
        // The digest a real client signs: stake_attestation_message(...). Its exact value doesn't
        // matter to the signer (it signs whatever digest it is handed under the given context).
        let msg = stake_attestation_message(b"test-net", epoch, target, daa, Hash64::default(), TransactionOutpoint::default());
        SignerRequest {
            request_id: req_id,
            validator_id: vid,
            purpose: SigningPurpose::Attestation,
            context: ATTESTATION_MLDSA87_CONTEXT.to_vec(),
            message_digest: SignerMessageDigest::Attestation(msg),
            metadata: SignerMetadata::Attestation { epoch, target_hash: target, target_daa_score: daa },
        }
    }

    #[test]
    fn permissive_signs_a_wellformed_transaction() {
        let k = key(0x11);
        let vid = k.validator_id;
        let mut s = SignerState::new(vec![k], SignerPolicy::Permissive, tmp_dir("perm-tx"), Hash::default()).unwrap();
        let req = SignerRequest {
            request_id: 1,
            validator_id: vid,
            purpose: SigningPurpose::Transaction,
            context: MLDSA87_TX_CONTEXT.to_vec(),
            message_digest: SignerMessageDigest::Transaction(Hash64::from_bytes([0xab; 64])),
            metadata: SignerMetadata::None,
        };
        let resp = s.handle_request(&req, Hash::default(), 1000);
        let sig = resp.result.expect("permissive signs a well-formed tx request");
        assert_eq!(sig.len(), 4627, "ML-DSA-87 signature length");
    }

    /// Audit M-04: a signed audit-log checkpoint verifies against the recomputed chain, and
    /// verification flags (a) an unknown/forged signature key and (b) a rewritten/truncated log.
    #[test]
    fn audit_checkpoint_roundtrip_and_tamper_detection_m04() {
        let dir = tmp_dir("m04-ckpt");
        let k = key(0x33);
        let vid = k.validator_id;
        let mut s = SignerState::new(vec![k], SignerPolicy::Permissive, dir.clone(), Hash::default()).unwrap();

        // Append three audit records (well-formed tx signs), then take a checkpoint at record_index 3.
        for i in 0u8..3 {
            let req = SignerRequest {
                request_id: i as u64 + 1,
                validator_id: vid,
                purpose: SigningPurpose::Transaction,
                context: MLDSA87_TX_CONTEXT.to_vec(),
                message_digest: SignerMessageDigest::Transaction(Hash64::from_bytes([0xa0 + i; 64])),
                metadata: SignerMetadata::None,
            };
            assert!(s.handle_request(&req, Hash::default(), 1000 + i as u64).result.is_ok());
        }
        let head_before = s.audit_chain_head();
        s.write_audit_checkpoint(5000).expect("checkpoint write");

        let audit_log = dir.join("audit.log");
        let ckpt_path = dir.join("audit.checkpoints");
        assert!(ckpt_path.exists(), "checkpoint file written");

        // The persisted checkpoint records record_index 3 and the current head.
        let ckpts = read_audit_checkpoints(&ckpt_path).unwrap();
        assert_eq!(ckpts.len(), 1);
        assert_eq!(ckpts[0].record_index, 3);
        assert_eq!(ckpts[0].chain_head, head_before);
        assert_eq!(ckpts[0].validator_id, vid);
        assert_eq!(ckpts[0].signature.len(), 4627, "full ML-DSA-87 checkpoint signature");

        // (clean) verifies with zero anomalies against the known key.
        let keys_full: HashMap<Hash64, ValidatorKey> = HashMap::from([(vid, key(0x33))]);
        let clean = verify_audit_checkpoints(&audit_log, &ckpt_path, &keys_full).unwrap();
        assert_eq!((clean.total, clean.anomalies), (1, 0), "clean checkpoint verifies");
        assert_eq!(clean.latest_head, head_before);

        // (a) signature/unknown-key anomaly: no key for this validator_id.
        let no_key: HashMap<Hash64, ValidatorKey> = HashMap::new();
        let unknown = verify_audit_checkpoints(&audit_log, &ckpt_path, &no_key).unwrap();
        assert_eq!(unknown.anomalies, 1, "checkpoint signed by an unheld key is flagged");

        // Reloading the signer over the same dir runs verification without panicking and recovers the
        // head (the checkpoint chain is intact).
        let s2 = SignerState::new(vec![key(0x33)], SignerPolicy::Permissive, dir.clone(), Hash::default()).unwrap();
        assert_eq!(s2.audit_chain_head(), head_before, "reload recovers the audit chain head");
        drop(s2);

        // (b) head-consistency anomaly: truncate the audit log below the checkpoint's record_index.
        fs::write(&audit_log, b"").unwrap();
        let tampered = verify_audit_checkpoints(&audit_log, &ckpt_path, &keys_full).unwrap();
        assert_eq!((tampered.total, tampered.anomalies), (1, 1), "deleting records below a checkpoint is detected");
    }

    /// Audit C-02 (Critical): a request must not be able to sign under another
    /// operation's cryptographic context. The cross-purpose forgery —
    /// purpose=Unbond + Unbond(attestation_digest) + context=ATTESTATION — would
    /// otherwise yield a valid attestation signature while SKIPPING the
    /// attestation equivocation guard. Even a STRICT signer must refuse it.
    #[test]
    fn rejects_cross_purpose_context_borrowing_c02() {
        use kaspa_consensus_core::dns_finality::UNBOND_REQUEST_CONTEXT;
        let k = key(0x42);
        let vid = k.validator_id;
        let mut s = SignerState::new(vec![k], SignerPolicy::Strict, tmp_dir("c02"), Hash::default()).unwrap();
        let att_msg = stake_attestation_message(
            b"test-net",
            7,
            Hash64::from_bytes([0x55; 64]),
            100,
            Hash64::default(),
            TransactionOutpoint::default(),
        );

        // Attack A: Unbond purpose borrowing the ATTESTATION context.
        let forged = SignerRequest {
            request_id: 1,
            validator_id: vid,
            purpose: SigningPurpose::Unbond,
            context: ATTESTATION_MLDSA87_CONTEXT.to_vec(),
            message_digest: SignerMessageDigest::Unbond(att_msg),
            metadata: SignerMetadata::None,
        };
        assert!(
            s.handle_request(&forged, Hash::default(), 1000).result.is_err(),
            "C-02: Unbond may not sign under the attestation context"
        );

        // Attack B (reverse): Transaction borrowing an overlay context.
        let forged2 = SignerRequest {
            request_id: 2,
            validator_id: vid,
            purpose: SigningPurpose::Transaction,
            context: UNBOND_REQUEST_CONTEXT.to_vec(),
            message_digest: SignerMessageDigest::Transaction(Hash64::from_bytes([0x66; 64])),
            metadata: SignerMetadata::None,
        };
        assert!(
            s.handle_request(&forged2, Hash::default(), 1000).result.is_err(),
            "C-02: Transaction may not borrow an overlay context"
        );

        // A well-formed Unbond under ITS OWN context still signs.
        let legit = SignerRequest {
            request_id: 3,
            validator_id: vid,
            purpose: SigningPurpose::Unbond,
            context: UNBOND_REQUEST_CONTEXT.to_vec(),
            message_digest: SignerMessageDigest::Unbond(Hash::from_bytes([0x77; 32])),
            metadata: SignerMetadata::None,
        };
        assert!(
            s.handle_request(&legit, Hash::default(), 1000).result.is_ok(),
            "a well-formed Unbond under its own context still signs"
        );
    }

    #[test]
    fn rejects_purpose_digest_mismatch() {
        let k = key(0x22);
        let vid = k.validator_id;
        let mut s = SignerState::new(vec![k], SignerPolicy::Permissive, tmp_dir("mismatch"), Hash::default()).unwrap();
        // purpose=Transaction but the digest is an Attestation variant → malformed.
        let req = SignerRequest {
            request_id: 1,
            validator_id: vid,
            purpose: SigningPurpose::Transaction,
            context: MLDSA87_TX_CONTEXT.to_vec(),
            message_digest: SignerMessageDigest::Attestation(Hash::default()),
            metadata: SignerMetadata::None,
        };
        assert!(matches!(s.handle_request(&req, Hash::default(), 1).result, Err(SignerError::PolicyViolation(_))));
    }

    #[test]
    fn rejects_oversized_context_without_panicking() {
        // audit F8: a >255-byte ML-DSA context must be refused in-band, not panic the signer (which
        // would poison the shared state mutex and wedge every subsequent request).
        let k = key(0x91);
        let vid = k.validator_id;
        let mut s = SignerState::new(vec![k], SignerPolicy::Permissive, tmp_dir("ctx-len"), Hash::default()).unwrap();
        let req = SignerRequest {
            request_id: 1,
            validator_id: vid,
            purpose: SigningPurpose::Transaction,
            context: vec![0u8; 256], // one over the FIPS-204 limit
            message_digest: SignerMessageDigest::Transaction(Hash64::from_bytes([0xcd; 64])),
            metadata: SignerMetadata::None,
        };
        assert!(matches!(s.handle_request(&req, Hash::default(), 1).result, Err(SignerError::PolicyViolation(_))));
        // The engine still signs a subsequent well-formed request (no poisoning / lasting damage).
        let ok = SignerRequest {
            request_id: 2,
            validator_id: vid,
            purpose: SigningPurpose::Transaction,
            context: MLDSA87_TX_CONTEXT.to_vec(),
            message_digest: SignerMessageDigest::Transaction(Hash64::from_bytes([0xce; 64])),
            metadata: SignerMetadata::None,
        };
        assert!(s.handle_request(&ok, Hash::default(), 2).result.is_ok(), "signer still works after refusing an oversized context");
    }

    #[test]
    fn denied_purpose_is_refused() {
        // F7 optional policy hook: a validator-only signer can deny Transaction signing.
        let k = key(0x95);
        let vid = k.validator_id;
        let mut s = SignerState::new(vec![k], SignerPolicy::Permissive, tmp_dir("deny-purpose"), Hash::default()).unwrap();
        s.set_denied_purposes(vec![SigningPurpose::Transaction]);
        let tx = SignerRequest {
            request_id: 1,
            validator_id: vid,
            purpose: SigningPurpose::Transaction,
            context: MLDSA87_TX_CONTEXT.to_vec(),
            message_digest: SignerMessageDigest::Transaction(Hash64::from_bytes([0xab; 64])),
            metadata: SignerMetadata::None,
        };
        assert!(matches!(s.handle_request(&tx, Hash::default(), 1).result, Err(SignerError::PolicyViolation(_))));
        // A non-denied purpose (Attestation) still signs.
        assert!(s.handle_request(&att_request(2, vid, 5, Hash64::from_bytes([0x01; 64]), 50), Hash::default(), 2).result.is_ok());
    }

    #[test]
    fn rejects_unknown_validator_key() {
        let mut s = SignerState::new(vec![key(0x33)], SignerPolicy::Permissive, tmp_dir("unknown"), Hash::default()).unwrap();
        let req = att_request(1, Hash64::from_bytes([0x99; 64]), 5, Hash64::from_bytes([0x01; 64]), 50);
        assert!(matches!(s.handle_request(&req, Hash::default(), 1).result, Err(SignerError::KeyNotFound)));
    }

    #[test]
    fn strict_blocks_equivocation_allows_rebroadcast() {
        let k = key(0x44);
        let vid = k.validator_id;
        let mut s = SignerState::new(vec![k], SignerPolicy::Strict, tmp_dir("strict"), Hash::default()).unwrap();
        let (t_x, t_y) = (Hash64::from_bytes([0x0a; 64]), Hash64::from_bytes([0x0b; 64]));
        // First attestation for epoch 7 → signed.
        assert!(s.handle_request(&att_request(1, vid, 7, t_x, 70), Hash::default(), 1).result.is_ok(), "first epoch-7 sign");
        // SAME target again → rebroadcast, still signed (not equivocation).
        assert!(s.handle_request(&att_request(2, vid, 7, t_x, 70), Hash::default(), 2).result.is_ok(), "rebroadcast of same target");
        // DIFFERENT target for epoch 7 → equivocation → BLOCKED.
        let blocked = s.handle_request(&att_request(3, vid, 7, t_y, 71), Hash::default(), 3).result;
        assert!(matches!(blocked, Err(SignerError::PolicyViolation(_))), "conflicting target for the same epoch is blocked");
        // A different epoch is unaffected.
        assert!(s.handle_request(&att_request(4, vid, 8, t_y, 80), Hash::default(), 4).result.is_ok(), "different epoch signs");
    }

    #[test]
    fn equivocation_guard_survives_restart() {
        let k = key(0x55);
        let vid = k.validator_id;
        let dir = tmp_dir("restart");
        let (t_x, t_y) = (Hash64::from_bytes([0x1a; 64]), Hash64::from_bytes([0x1b; 64]));
        {
            let mut s = SignerState::new(vec![k], SignerPolicy::Strict, dir.clone(), Hash::default()).unwrap();
            assert!(s.handle_request(&att_request(1, vid, 9, t_x, 90), Hash::default(), 1).result.is_ok());
        }
        // CRASH + RESTART: a fresh engine over the same state dir must remember epoch 9 and still
        // block a conflicting target (the equivocation log is fsync'd + reloaded).
        let mut s2 = SignerState::new(vec![key(0x55)], SignerPolicy::Strict, dir, Hash::default()).unwrap();
        let blocked = s2.handle_request(&att_request(2, vid, 9, t_y, 91), Hash::default(), 2).result;
        assert!(matches!(blocked, Err(SignerError::PolicyViolation(_))), "equivocation guard persists across restart");
    }

    #[test]
    fn audit_log_chains_and_persists() {
        let k = key(0x66);
        let vid = k.validator_id;
        let dir = tmp_dir("audit");
        let head_after_two = {
            let mut s = SignerState::new(vec![k], SignerPolicy::Permissive, dir.clone(), Hash::default()).unwrap();
            assert_eq!(s.audit_chain_head(), Hash64::default(), "empty log starts at the zero chain head");
            s.handle_request(&att_request(1, vid, 1, Hash64::from_bytes([0x01; 64]), 10), Hash::default(), 100);
            let h1 = s.audit_chain_head();
            assert_ne!(h1, Hash64::default(), "chain head advances after a record");
            s.handle_request(&att_request(2, vid, 2, Hash64::from_bytes([0x02; 64]), 20), Hash::default(), 200);
            let h2 = s.audit_chain_head();
            assert_ne!(h2, h1, "chain head advances again");
            h2
        };
        // Reload: replaying the persisted log recomputes the identical chain head (tamper-evidence).
        let s2 = SignerState::new(vec![key(0x66)], SignerPolicy::Permissive, dir, Hash::default()).unwrap();
        assert_eq!(s2.audit_chain_head(), head_after_two, "reloaded audit chain head matches");
    }

    /// End-to-end over a REAL Unix domain socket: handshake + length-prefixed Borsh framing + a sign
    /// and an equivocation refusal flow through `transport::serve_connection` exactly as the daemon
    /// binary runs it. Proves the daemon actually serves, not just that `handle_request` works.
    #[cfg(unix)]
    #[test]
    fn socket_roundtrip_signs_then_refuses_equivocation() {
        use super::transport::{SignerClient, serve_connection};
        use std::os::unix::net::UnixListener;
        use std::sync::{Arc, Mutex};

        let k = key(0x77);
        let vid = k.validator_id;
        let server_id = Hash::from_bytes([0x5e; 32]);
        let state = Arc::new(Mutex::new(SignerState::new(vec![k], SignerPolicy::Strict, tmp_dir("socket"), server_id).unwrap()));

        let sock = std::env::temp_dir().join("kaspa-pq-signer-test-77.sock");
        let _ = fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).unwrap();
        let srv_state = Arc::clone(&state);
        let server = std::thread::spawn(move || {
            for conn in listener.incoming().take(1) {
                if let Ok(stream) = conn {
                    serve_connection(stream, &srv_state, server_id, &[]);
                }
            }
        });

        // Connect through the public client API (the validator's production path).
        let mut client = SignerClient::connect(&sock, Hash::from_bytes([0xc1; 32])).unwrap();
        assert_eq!(client.server_identity, server_id, "handshake ack carries the daemon identity");

        // First attestation → signed over the wire.
        let resp1 = client.sign(&att_request(1, vid, 42, Hash64::from_bytes([0xaa; 64]), 420)).unwrap();
        assert_eq!(resp1.result.unwrap().len(), 4627, "socket signs the first attestation");

        // Conflicting attestation (same epoch, different target) → equivocation refusal over the wire.
        let resp2 = client.sign(&att_request(2, vid, 42, Hash64::from_bytes([0xbb; 64]), 421)).unwrap();
        assert!(matches!(resp2.result, Err(SignerError::PolicyViolation(_))), "socket refuses the equivocating attestation");

        drop(client);
        server.join().unwrap();
        let _ = fs::remove_file(&sock);
    }
}
