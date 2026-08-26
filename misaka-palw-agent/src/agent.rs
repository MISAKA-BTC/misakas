//! `palw-agent` — the VPS runtime supervisor for the pinned `palw-worker`
//! (docs/misaka-palw-vps-canonical-worker-design-v0.1-ja.md §4.2, §5, §8; Phase A).
//!
//! One Unix domain socket, transport `misaka-palw-agent-borsh/v1`: per connection, one framed
//! [`PalwAgentRequestV1`] in, one framed [`PalwAgentResponseV1`] out. The agent supervises the
//! runtime and decides NOTHING about the meaning of a computation:
//!
//! * **Boot gate.** At startup it probes the worker's v2 manifest and — unless explicitly run
//!   with `--allow-ungated` for development — requires a registered golden set
//!   (`MISAKA_PALW_GOLDEN`) whose `v2-selftest` passes. A selftest failure QUARANTINES the
//!   agent: it stays up, answers health probes, and rejects every job until an operator
//!   intervenes. Abstaining beats answering on a runtime that failed its own class's vectors
//!   (refutation-dominant rule, §13).
//! * **Admission control.** Envelope shape, runtime-identity equality, deadline feasibility,
//!   duplicate-job suppression and the single Phase-A job slot are all checked BEFORE a worker
//!   process (and its 1.2 GB model load) is spawned.
//! * **Supervision.** Per-job process, stdin closed after one frame, both pipes drained on
//!   threads (the pipe-buffer deadlock lesson), kill on timeout or on the job's own deadline,
//!   and NO partial output ever forwarded.
//! * **Response verification.** The worker's framed result is re-parsed and re-bound before it
//!   leaves the agent: request-hash echo, job id, token counts, recomputed CU, and the full
//!   `job_context_hash` re-derived by the agent itself. A worker that returns a result for a
//!   different job — or a plausible-but-misbound one — is a `JobFailed`, not a response.
//!
//! Prohibitions (§4.2): the agent never rewrites prompts, never injects expected outputs, never
//! retries onto a different runtime, and never caches results. It holds no validator keys.

use std::collections::{HashSet, VecDeque};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};

use kaspa_consensus_core::palw_v2::{
    PALW_JOB_WIRE_VERSION_V2, PALW_V2_MAX_FRAME_BYTES, PalwAgentHealthV1, PalwAgentRequestV1, PalwAgentResponseV1, PalwAgentStateV1,
    PalwJobContextV2, PalwJobEnvelopeV2, PalwJobResultV2, PalwStopReasonV2, canonical_compute_units_v2, decode_framed_borsh,
    job_request_hash_v2, read_framed, write_framed,
};
use kaspa_hashes::Hash64;

const DEDUPE_WINDOW: usize = 1024;
const STDERR_TAIL_BYTES: usize = 4096;
const MANIFEST_PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const SELFTEST_TIMEOUT: Duration = Duration::from_secs(1800);

fn die(msg: String) -> ! {
    eprintln!("[palw-agent] {msg}");
    std::process::exit(1);
}

// ---------------------------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------------------------

struct AgentConfig {
    listen: PathBuf,
    worker: PathBuf,
    job_timeout: Duration,
    /// Admission estimate: a job whose deadline is closer than this cannot finish; reject it
    /// without burning a model load (§8.3 "deadlineまでの残時間がworst-case estimate未満").
    worst_case_job_ms: u64,
    allow_ungated: bool,
    max_conns: usize,
}

fn parse_args() -> AgentConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut listen: Option<String> = None;
    let mut worker: Option<String> = None;
    let mut job_timeout_secs: u64 = 300;
    let mut worst_case_job_ms: u64 = 60_000;
    let mut allow_ungated = false;
    let mut max_conns: usize = 8;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => {
                i += 1;
                listen = args.get(i).cloned();
            }
            "--worker" => {
                i += 1;
                worker = args.get(i).cloned();
            }
            "--job-timeout-secs" => {
                i += 1;
                job_timeout_secs =
                    args.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(|| die("--job-timeout-secs needs a number".into()));
            }
            "--worst-case-job-ms" => {
                i += 1;
                worst_case_job_ms =
                    args.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(|| die("--worst-case-job-ms needs a number".into()));
            }
            "--allow-ungated" => allow_ungated = true,
            "--max-conns" => {
                i += 1;
                max_conns = args.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(|| die("--max-conns needs a number".into()));
            }
            other => die(format!("unknown argument {other:?}")),
        }
        i += 1;
    }
    let listen = listen.unwrap_or_else(|| die("--listen <socket path> is required".into()));
    let worker = worker.unwrap_or_else(|| die("--worker <palw-worker path> is required".into()));
    AgentConfig {
        listen: PathBuf::from(listen),
        worker: PathBuf::from(worker),
        job_timeout: Duration::from_secs(job_timeout_secs.max(1)),
        worst_case_job_ms,
        allow_ungated,
        max_conns: max_conns.clamp(1, 64),
    }
}

// ---------------------------------------------------------------------------------------------
// Worker identity probe (v2-manifest) and boot selftest gate
// ---------------------------------------------------------------------------------------------

/// The identity this agent fronts, read once at boot from `--mode v2-manifest`. Every job is
/// pre-checked against it so an envelope for another runtime is rejected without a spawn; the
/// worker re-checks on its own (defense in depth, not a substitute).
struct WorkerIdentity {
    runtime_manifest_hash: Hash64,
    model_profile_id: Hash64,
    runtime_class_id: Hash64,
    shape_profile_id: Hash64,
    trace_scheme_id: Hash64,
    cu_ruleset_id: Hash64,
    tokenizer_id: Hash64,
    golden_vector_root: Hash64,
    golden_registered: bool,
    max_context_tokens: u32,
}

fn run_captured(cmd: &mut Command, timeout: Duration, what: &str) -> (std::process::ExitStatus, Vec<u8>, Vec<u8>) {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| die(format!("cannot spawn {what}: {e}")));
    let mut stdout_pipe = child.stdout.take().expect("piped");
    let mut stderr_pipe = child.stderr.take().expect("piped");
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    die(format!("{what} exceeded its {timeout:?} timeout"));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => die(format!("waiting on {what} failed: {e}")),
        }
    };
    (status, out_thread.join().unwrap_or_default(), err_thread.join().unwrap_or_default())
}

fn manifest_hash64(doc: &serde_json::Value, key: &str) -> Hash64 {
    let hex_str = doc.get(key).and_then(|v| v.as_str()).unwrap_or_else(|| die(format!("worker manifest lacks {key}")));
    let mut out = [0u8; 64];
    if hex_str.len() != 128 || faster_hex::hex_decode(hex_str.as_bytes(), &mut out).is_err() {
        die(format!("worker manifest field {key} is not 128 hex chars"));
    }
    Hash64::from_bytes(out)
}

fn probe_worker_identity(cfg: &AgentConfig) -> WorkerIdentity {
    let (status, stdout, stderr) =
        run_captured(Command::new(&cfg.worker).args(["--mode", "v2-manifest"]), MANIFEST_PROBE_TIMEOUT, "worker v2-manifest probe");
    if !status.success() {
        die(format!("worker v2-manifest probe failed: {}", String::from_utf8_lossy(&stderr)));
    }
    let doc: serde_json::Value =
        serde_json::from_slice(&stdout).unwrap_or_else(|e| die(format!("worker v2-manifest output is not JSON: {e}")));
    if doc.get("fp_environment_canonical").and_then(|v| v.as_bool()) != Some(true) {
        die("worker reports a non-canonical floating-point environment — refusing to serve".into());
    }
    WorkerIdentity {
        runtime_manifest_hash: manifest_hash64(&doc, "runtime_manifest_hash_v2"),
        model_profile_id: manifest_hash64(&doc, "model_profile_id"),
        runtime_class_id: manifest_hash64(&doc, "runtime_class_id"),
        shape_profile_id: manifest_hash64(&doc, "shape_profile_id_v2"),
        trace_scheme_id: manifest_hash64(&doc, "trace_scheme_id_v2"),
        cu_ruleset_id: manifest_hash64(&doc, "cu_ruleset_id_v2"),
        tokenizer_id: manifest_hash64(&doc, "tokenizer_id_v2"),
        golden_vector_root: manifest_hash64(&doc, "golden_vector_root"),
        golden_registered: doc.get("golden_registered").and_then(|v| v.as_bool()).unwrap_or(false),
        max_context_tokens: doc
            .get("max_context_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| die("worker manifest lacks max_context_tokens".into())) as u32,
    }
}

// ---------------------------------------------------------------------------------------------
// Agent state
// ---------------------------------------------------------------------------------------------

struct AgentState {
    cfg: AgentConfig,
    identity: WorkerIdentity,
    quarantined: bool,
    selftest_passed: bool,
    /// The single Phase-A execution slot. `try_lock` failure IS the busy signal; there is no
    /// hidden queue whose depth could silently eat deadlines.
    job_slot: Mutex<()>,
    recent_jobs: Mutex<(VecDeque<Hash64>, HashSet<Hash64>)>,
    conns: AtomicUsize,
    jobs_total: AtomicU64,
    jobs_ok: AtomicU64,
    jobs_rejected: AtomicU64,
    jobs_failed: AtomicU64,
    timeouts_total: AtomicU64,
}

impl AgentState {
    fn health(&self) -> PalwAgentHealthV1 {
        let state = if self.quarantined {
            PalwAgentStateV1::Quarantined
        } else {
            match self.job_slot.try_lock() {
                Ok(_guard) => PalwAgentStateV1::Ready,
                Err(TryLockError::WouldBlock) => PalwAgentStateV1::Busy,
                Err(TryLockError::Poisoned(_)) => PalwAgentStateV1::Quarantined,
            }
        };
        PalwAgentHealthV1 {
            state,
            selftest_passed: self.selftest_passed,
            runtime_manifest_hash: self.identity.runtime_manifest_hash,
            golden_vector_root: self.identity.golden_vector_root,
            max_context_tokens: self.identity.max_context_tokens,
            jobs_total: self.jobs_total.load(Ordering::Relaxed),
            jobs_ok: self.jobs_ok.load(Ordering::Relaxed),
            jobs_rejected: self.jobs_rejected.load(Ordering::Relaxed),
            jobs_failed: self.jobs_failed.load(Ordering::Relaxed),
            timeouts_total: self.timeouts_total.load(Ordering::Relaxed),
        }
    }

    fn seen_before_or_record(&self, job_id: Hash64) -> bool {
        let mut guard = self.recent_jobs.lock().unwrap_or_else(|p| p.into_inner());
        let (order, set) = &mut *guard;
        if set.contains(&job_id) {
            return true;
        }
        order.push_back(job_id);
        set.insert(job_id);
        if order.len() > DEDUPE_WINDOW
            && let Some(evicted) = order.pop_front()
        {
            set.remove(&evicted);
        }
        false
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// Peer credentials: the socket's 0660 mode is the primary gate; this is the second lock. Same
// effective uid only, for Phase A (a group policy is a fleet-deployment decision).
// ---------------------------------------------------------------------------------------------

fn peer_is_authorized(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let fd = stream.as_raw_fd();
    #[cfg(target_os = "linux")]
    {
        let mut cred = libc::ucred { pid: 0, uid: u32::MAX, gid: u32::MAX };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let rc =
            unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len) };
        rc == 0 && cred.uid == unsafe { libc::geteuid() }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut uid: libc::uid_t = u32::MAX;
        let mut gid: libc::gid_t = u32::MAX;
        let rc = unsafe { libc::getpeereid(fd, &mut uid, &mut gid) };
        rc == 0 && uid == unsafe { libc::geteuid() }
    }
}

// ---------------------------------------------------------------------------------------------
// Job handling
// ---------------------------------------------------------------------------------------------

fn reject(state: &AgentState, code: &str, message: String) -> PalwAgentResponseV1 {
    state.jobs_rejected.fetch_add(1, Ordering::Relaxed);
    eprintln!("[palw-agent] job rejected: {code}: {message}");
    PalwAgentResponseV1::JobRejected { code: code.to_string(), message }
}

fn failed(state: &AgentState, code: &str, message: String) -> PalwAgentResponseV1 {
    state.jobs_failed.fetch_add(1, Ordering::Relaxed);
    eprintln!("[palw-agent] job failed: {code}: {message}");
    PalwAgentResponseV1::JobFailed { code: code.to_string(), message }
}

fn handle_job(state: &AgentState, envelope: PalwJobEnvelopeV2) -> PalwAgentResponseV1 {
    state.jobs_total.fetch_add(1, Ordering::Relaxed);
    if state.quarantined {
        return reject(state, "quarantined", "this runtime failed its conformance gate; operator intervention required".into());
    }
    if let Err(e) = envelope.validate_shape(state.identity.max_context_tokens) {
        return reject(state, "invalid_envelope", e.to_string());
    }
    let id = &state.identity;
    let identity_checks: [(&str, Hash64, Hash64); 6] = [
        ("model_profile_id", id.model_profile_id, envelope.model_profile_id),
        ("runtime_manifest_hash", id.runtime_manifest_hash, envelope.runtime_manifest_hash),
        ("runtime_class_id", id.runtime_class_id, envelope.runtime_class_id),
        ("shape_profile_id", id.shape_profile_id, envelope.shape_profile_id),
        ("trace_scheme_id", id.trace_scheme_id, envelope.trace_scheme_id),
        ("cu_ruleset_id", id.cu_ruleset_id, envelope.cu_ruleset_id),
    ];
    for (field, ours, declared) in identity_checks {
        if ours != declared {
            return reject(state, "runtime_identity_mismatch", format!("{field}: this runtime is not the one the envelope names"));
        }
    }
    if envelope.deadline_unix_ms != 0 {
        let now = now_unix_ms();
        if now.saturating_add(state.cfg.worst_case_job_ms) >= envelope.deadline_unix_ms {
            return reject(
                state,
                "deadline_unreachable",
                format!("deadline {} is within the {}ms worst-case estimate", envelope.deadline_unix_ms, state.cfg.worst_case_job_ms),
            );
        }
    }
    if state.seen_before_or_record(envelope.job_id) {
        return reject(state, "duplicate_job", "this job_id was already admitted in the recent window".into());
    }
    let _slot = match state.job_slot.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => return reject(state, "busy", "the Phase-A execution slot is occupied".into()),
        Err(TryLockError::Poisoned(_)) => return reject(state, "quarantined", "execution slot poisoned by a previous panic".into()),
    };
    run_worker_job(state, &envelope)
}

fn run_worker_job(state: &AgentState, envelope: &PalwJobEnvelopeV2) -> PalwAgentResponseV1 {
    // Canonical re-encoding: what the worker hashes as its request is exactly what any client
    // can recompute from the envelope alone (Borsh is deterministic).
    let payload = match borsh::to_vec(envelope) {
        Ok(bytes) => bytes,
        Err(e) => return failed(state, "encode", e.to_string()),
    };
    let expected_request_hash = job_request_hash_v2(&payload);

    let mut child = match Command::new(&state.cfg.worker)
        .args(["--mode", "v2-job"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return failed(state, "spawn", format!("cannot spawn the worker: {e}")),
    };

    // One frame in, then EOF: the worker treats trailing bytes as an error, and an open stdin
    // as a hang. Drain both pipes on threads BEFORE waiting (llama's model-load stderr alone
    // can overflow an OS pipe buffer).
    {
        let mut stdin = child.stdin.take().expect("piped");
        if let Err(e) = write_framed(&mut stdin, &payload) {
            let _ = child.kill();
            let _ = child.wait();
            return failed(state, "stdin", format!("cannot hand the job to the worker: {e}"));
        }
    }
    let mut stdout_pipe = child.stdout.take().expect("piped");
    let mut stderr_pipe = child.stderr.take().expect("piped");
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    // The effective ceiling is the smaller of the configured timeout and the job's own
    // deadline: a result past its deadline is a result nobody may use (partial results are
    // discarded, per §13 — kill, do not salvage).
    let mut ceiling = state.cfg.job_timeout;
    if envelope.deadline_unix_ms != 0 {
        let remaining_ms = envelope.deadline_unix_ms.saturating_sub(now_unix_ms());
        ceiling = ceiling.min(Duration::from_millis(remaining_ms));
    }
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() > ceiling {
                    let _ = child.kill();
                    let _ = child.wait();
                    state.timeouts_total.fetch_add(1, Ordering::Relaxed);
                    // The drain threads see EOF once the process dies; their buffers are dropped.
                    let _ = out_thread.join();
                    let _ = err_thread.join();
                    return failed(state, "timeout", format!("worker exceeded {ceiling:?}; killed, partial output discarded"));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return failed(state, "wait", e.to_string());
            }
        }
    };
    let stdout_bytes = out_thread.join().unwrap_or_default();
    let stderr_bytes = err_thread.join().unwrap_or_default();

    if !status.success() {
        let tail_start = stderr_bytes.len().saturating_sub(STDERR_TAIL_BYTES);
        return failed(
            state,
            "worker_exit",
            format!("worker exited with {status}: {}", String::from_utf8_lossy(&stderr_bytes[tail_start..])),
        );
    }

    // Re-parse and re-bind the result before it leaves this process.
    let mut stdout_slice: &[u8] = &stdout_bytes;
    let result_payload = match read_framed(&mut stdout_slice, PALW_V2_MAX_FRAME_BYTES) {
        Ok(payload) => payload,
        Err(e) => return failed(state, "malformed_result", format!("worker stdout is not one canonical frame: {e}")),
    };
    let result: PalwJobResultV2 = match decode_framed_borsh(&result_payload) {
        Ok(result) => result,
        Err(e) => return failed(state, "malformed_result", e.to_string()),
    };
    if result.version != PALW_JOB_WIRE_VERSION_V2 {
        return failed(state, "malformed_result", format!("result version {} is not v2", result.version));
    }
    if result.request_hash != expected_request_hash {
        return failed(state, "response_binding", "the result does not echo this request's hash".into());
    }
    if result.job_id != envelope.job_id {
        return failed(state, "response_binding", "the result names a different job".into());
    }
    let projection = &result.projection;
    let expected_context = PalwJobContextV2::from_envelope(envelope, state.identity.tokenizer_id).context_hash();
    if projection.job_context_hash != expected_context {
        return failed(state, "response_binding", "the result's job context is not this job's context".into());
    }
    if projection.prefill_tokens != envelope.declared_prefill_tokens()
        || projection.decode_tokens != envelope.exact_decode_tokens
        || projection.trace_event_count != envelope.exact_decode_tokens
    {
        return failed(state, "response_binding", "the result's token counts contradict the job's exact budgets".into());
    }
    if projection.canonical_compute_units
        != canonical_compute_units_v2(envelope.declared_prefill_tokens(), envelope.exact_decode_tokens)
    {
        return failed(state, "response_binding", "the result's CU does not re-derive from the canonical ruleset".into());
    }
    if projection.stop_reason != PalwStopReasonV2::ExactBudgetReached {
        return failed(state, "response_binding", "only exact-budget completion bears a receipt in this profile".into());
    }

    state.jobs_ok.fetch_add(1, Ordering::Relaxed);
    eprintln!(
        "[palw-agent] job ok: prefill={} decode={} in {:?}; root={}…",
        projection.prefill_tokens,
        projection.decode_tokens,
        started.elapsed(),
        &faster_hex::hex_string(&projection.full_logits_trace_root.as_byte_slice()[..8])
    );
    PalwAgentResponseV1::JobOk(result)
}

fn handle_connection(state: &AgentState, mut stream: UnixStream) {
    if !peer_is_authorized(&stream) {
        eprintln!("[palw-agent] connection from an unauthorized peer uid; closing");
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let payload = match read_framed(&mut stream, PALW_V2_MAX_FRAME_BYTES) {
        Ok(payload) => payload,
        Err(e) => {
            eprintln!("[palw-agent] request rejected: {e}");
            return;
        }
    };
    let _ = stream.set_read_timeout(None);
    let response = match decode_framed_borsh::<PalwAgentRequestV1>(&payload) {
        Ok(PalwAgentRequestV1::Health) => PalwAgentResponseV1::Health(state.health()),
        Ok(PalwAgentRequestV1::Job(envelope)) => handle_job(state, envelope),
        Err(e) => {
            state.jobs_rejected.fetch_add(1, Ordering::Relaxed);
            PalwAgentResponseV1::JobRejected { code: "invalid_request".into(), message: e.to_string() }
        }
    };
    match borsh::to_vec(&response) {
        Ok(bytes) => {
            if let Err(e) = write_framed(&mut stream, &bytes) {
                eprintln!("[palw-agent] cannot write the response frame: {e}");
            }
        }
        Err(e) => eprintln!("[palw-agent] cannot serialize the response: {e}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------------------------

fn bind_socket(path: &Path) -> UnixListener {
    if path.exists() {
        // A stale socket from a dead agent is re-bindable; a LIVE agent's socket is not ours to
        // steal — probe before unlinking.
        match UnixStream::connect(path) {
            Ok(_) => die(format!("another agent is already serving {}", path.display())),
            Err(_) => {
                std::fs::remove_file(path).unwrap_or_else(|e| die(format!("cannot remove stale socket {}: {e}", path.display())));
            }
        }
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| die(format!("cannot create {}: {e}", parent.display())));
    }
    let listener = UnixListener::bind(path).unwrap_or_else(|e| die(format!("cannot bind {}: {e}", path.display())));
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))
        .unwrap_or_else(|e| die(format!("cannot set 0660 on {}: {e}", path.display())));
    listener
}

pub fn run() {
    let cfg = parse_args();
    if !cfg.worker.is_file() {
        die(format!("worker binary not found at {}", cfg.worker.display()));
    }

    eprintln!("[palw-agent] probing worker identity ({})", cfg.worker.display());
    let identity = probe_worker_identity(&cfg);
    eprintln!(
        "[palw-agent] worker manifest {}… class {}… golden_registered={}",
        &faster_hex::hex_string(&identity.runtime_manifest_hash.as_byte_slice()[..8]),
        &faster_hex::hex_string(&identity.runtime_class_id.as_byte_slice()[..8]),
        identity.golden_registered
    );

    let (quarantined, selftest_passed) = if identity.golden_registered {
        eprintln!("[palw-agent] running the boot golden selftest (this loads the model per vector)");
        let (status, _stdout, stderr) =
            run_captured(Command::new(&cfg.worker).args(["--mode", "v2-selftest"]), SELFTEST_TIMEOUT, "worker v2-selftest");
        if status.success() {
            eprintln!("[palw-agent] golden selftest PASSED");
            (false, true)
        } else {
            let tail_start = stderr.len().saturating_sub(STDERR_TAIL_BYTES);
            eprintln!(
                "[palw-agent] golden selftest FAILED — entering QUARANTINE, all jobs will be rejected: {}",
                String::from_utf8_lossy(&stderr[tail_start..])
            );
            (true, false)
        }
    } else if cfg.allow_ungated {
        eprintln!("[palw-agent] WARNING: serving WITHOUT a registered golden set (--allow-ungated is a dev flag)");
        (false, false)
    } else {
        die(format!(
            "no golden set is registered (set {} on the worker environment); refusing to serve — pass --allow-ungated only for development",
            "MISAKA_PALW_GOLDEN"
        ));
    };

    let listener = bind_socket(&cfg.listen);
    eprintln!("[palw-agent] listening on {} (mode 0660, one request per connection)", cfg.listen.display());

    let state = Arc::new(AgentState {
        cfg,
        identity,
        quarantined,
        selftest_passed,
        job_slot: Mutex::new(()),
        recent_jobs: Mutex::new((VecDeque::with_capacity(DEDUPE_WINDOW), HashSet::with_capacity(DEDUPE_WINDOW))),
        conns: AtomicUsize::new(0),
        jobs_total: AtomicU64::new(0),
        jobs_ok: AtomicU64::new(0),
        jobs_rejected: AtomicU64::new(0),
        jobs_failed: AtomicU64::new(0),
        timeouts_total: AtomicU64::new(0),
    });

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!("[palw-agent] accept failed: {e}");
                continue;
            }
        };
        let state = Arc::clone(&state);
        if state.conns.fetch_add(1, Ordering::AcqRel) >= state.cfg.max_conns {
            state.conns.fetch_sub(1, Ordering::AcqRel);
            eprintln!("[palw-agent] connection cap reached; dropping a connection unanswered");
            continue;
        }
        std::thread::spawn(move || {
            handle_connection(&state, stream);
            state.conns.fetch_sub(1, Ordering::AcqRel);
        });
    }
}
