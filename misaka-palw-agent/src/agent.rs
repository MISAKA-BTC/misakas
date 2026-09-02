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
use misaka_palw::host_security::{
    ConfinementBackend, PALW_WORKER_ENV_ALLOWLIST, arm_memory_ceiling, attach_to_cgroup, confinement_backend_in_force,
    harden_worker_command, resident_bytes, worker_max_resident_bytes, worker_working_dir,
};

const DEDUPE_WINDOW: usize = 1024;
const STDERR_TAIL_BYTES: usize = 4096;
const MANIFEST_PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const SELFTEST_TIMEOUT: Duration = Duration::from_secs(1800);
/// How often the supervision loop asks what the child is resident for. Cheap on Linux (one
/// `/proc` read); a subprocess on macOS, which is why it is not every wake-up.
const RESIDENT_POLL: Duration = Duration::from_millis(500);

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
    /// ADR-0079 Decision 5: an explicit working directory for every worker child, which is
    /// neither the operator's home nor the node's datadir.
    workdir: PathBuf,
    /// ADR-0079 Decision 6 (as corrected by SA-1): the per-job RESIDENT ceiling. Exceeding it is
    /// a `JobFailed`; the supervisor keeps its socket, its slot and its seat.
    max_resident_bytes: u64,
}

fn parse_args() -> AgentConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut listen: Option<String> = None;
    let mut worker: Option<String> = None;
    let mut job_timeout_secs: u64 = 300;
    let mut worst_case_job_ms: u64 = 60_000;
    let mut allow_ungated = false;
    let mut max_conns: usize = 8;
    let mut workdir: Option<String> = None;
    let mut max_resident_bytes: Option<u64> = None;
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
            "--workdir" => {
                i += 1;
                workdir = args.get(i).cloned();
            }
            "--worker-max-resident-bytes" => {
                i += 1;
                max_resident_bytes = Some(
                    args.get(i)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(|| die("--worker-max-resident-bytes needs a number".into())),
                );
            }
            other => die(format!("unknown argument {other:?}")),
        }
        i += 1;
    }
    let listen = listen.unwrap_or_else(|| die("--listen <socket path> is required".into()));
    let worker = worker.unwrap_or_else(|| die("--worker <palw-worker path> is required".into()));
    if let Some(dir) = &workdir {
        // SAFETY: single-threaded argument parsing, before any spawn or thread exists.
        unsafe { std::env::set_var(misaka_palw::host_security::PALW_WORKER_WORKDIR_ENV, dir) };
    }
    let workdir = match worker_working_dir(None) {
        Ok(dir) => dir,
        Err(e) => die(e),
    };
    AgentConfig {
        listen: PathBuf::from(listen),
        worker: PathBuf::from(worker),
        job_timeout: Duration::from_secs(job_timeout_secs.max(1)),
        worst_case_job_ms,
        allow_ungated,
        max_conns: max_conns.clamp(1, 64),
        workdir,
        max_resident_bytes: max_resident_bytes.filter(|v| *v > 0).unwrap_or_else(worker_max_resident_bytes),
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

/// Run a worker sub-command to completion under the SAME spawn discipline as a job (ADR-0079
/// Decision 5): `env_clear()`, the allowlist, the pinned working directory. The manifest probe and
/// the boot selftest are worker processes too — a discipline that only covers the job spawn is a
/// discipline with two boot-time holes in it.
fn run_captured(cmd: &mut Command, workdir: &Path, timeout: Duration, what: &str) -> (std::process::ExitStatus, Vec<u8>, Vec<u8>) {
    harden_worker_command(cmd, workdir);
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
    let (status, stdout, stderr) = run_captured(
        Command::new(&cfg.worker).args(["--mode", "v2-manifest"]),
        &cfg.workdir,
        MANIFEST_PROBE_TIMEOUT,
        "worker v2-manifest probe",
    );
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

/// Why the supervision loop stopped waiting. Every arm but `Exited` has already killed and reaped
/// the child: the supervisor decides how a job ends, and a job that ends any of these ways ends
/// as a `JobFailed`.
#[derive(Debug)]
enum SupervisionOutcome {
    Exited(std::process::ExitStatus),
    Timeout,
    /// ADR-0079 Decision 6, corrected by SA-1: the ceiling measures RESIDENT bytes, not address
    /// space — the hybrid class maps a 33 GiB artifact and an address-space cap would kill it
    /// while it was still mapping.
    MemoryCeiling {
        observed: u64,
        limit: u64,
        measure: misaka_palw::host_security::ResidentMeasure,
    },
    WaitFailed(String),
}

/// Wait for a worker child under BOTH ceilings — the wall-clock deadline the supervisor already
/// owned, and the resident-memory ceiling ADR-0079 Decision 6 adds.
///
/// Kept a free function taking a `Child` so a test can drive it with an ordinary process: the
/// property under test is "the supervisor kills and survives", and that property does not need a
/// model to be true.
fn supervise_child(child: &mut std::process::Child, deadline: Duration, max_resident: u64) -> SupervisionOutcome {
    let started = Instant::now();
    let mut last_probe = Instant::now();
    let pid = child.id();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return SupervisionOutcome::Exited(status),
            Ok(None) => {
                if started.elapsed() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return SupervisionOutcome::Timeout;
                }
                if max_resident > 0 && last_probe.elapsed() >= RESIDENT_POLL {
                    last_probe = Instant::now();
                    let (observed, measure) = resident_bytes(pid);
                    if let Some(observed) = observed
                        && observed > max_resident
                    {
                        let _ = child.kill();
                        let _ = child.wait();
                        return SupervisionOutcome::MemoryCeiling { observed, limit: max_resident, measure };
                    }
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return SupervisionOutcome::WaitFailed(e.to_string());
            }
        }
    }
}

fn run_worker_job(state: &AgentState, envelope: &PalwJobEnvelopeV2) -> PalwAgentResponseV1 {
    // Canonical re-encoding: what the worker hashes as its request is exactly what any client
    // can recompute from the envelope alone (Borsh is deterministic).
    let payload = match borsh::to_vec(envelope) {
        Ok(bytes) => bytes,
        Err(e) => return failed(state, "encode", e.to_string()),
    };
    let expected_request_hash = job_request_hash_v2(&payload);

    // ADR-0079 Decision 5/6: the job process starts with nothing the arithmetic does not need,
    // in a directory that is not the operator's, under a resident ceiling. The ceiling is armed
    // BEFORE the spawn so a delegated cgroup already carries `memory.max` when the child lands
    // in it.
    let ceiling_backend = arm_memory_ceiling(state.cfg.max_resident_bytes);
    let mut command = Command::new(&state.cfg.worker);
    command.args(["--mode", "v2-job"]);
    harden_worker_command(&mut command, &state.cfg.workdir);
    let mut child = match command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(e) => return failed(state, "spawn", format!("cannot spawn the worker: {e}")),
    };
    attach_to_cgroup(&ceiling_backend, child.id());

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
    let status = match supervise_child(&mut child, ceiling, state.cfg.max_resident_bytes) {
        SupervisionOutcome::Exited(status) => status,
        SupervisionOutcome::Timeout => {
            state.timeouts_total.fetch_add(1, Ordering::Relaxed);
            // The drain threads see EOF once the process dies; their buffers are dropped.
            let _ = out_thread.join();
            let _ = err_thread.join();
            return failed(state, "timeout", format!("worker exceeded {ceiling:?}; killed, partial output discarded"));
        }
        SupervisionOutcome::MemoryCeiling { observed, limit, measure } => {
            let _ = out_thread.join();
            let _ = err_thread.join();
            // ADR-0079 Decision 6 / S9: a FAILED JOB, never a dead node. The slot is released by
            // the caller's guard, the socket is still bound, and this agent answers the next
            // request. An OOM killer reaping the node instead would be an availability attack
            // that costs the attacker one prompt.
            return failed(
                state,
                "memory_ceiling",
                format!(
                    "worker resident {observed} bytes exceeded the {limit}-byte ceiling ({} via {}); killed, partial output discarded",
                    ceiling_backend.name(),
                    measure.name()
                ),
            );
        }
        SupervisionOutcome::WaitFailed(e) => return failed(state, "wait", e),
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
            // **Half-close, because the protocol says the sender does** — `read_framed`'s own doc:
            // "the SENDER half-closes its write side (`shutdown(SHUT_WR)`) after its frame, so the
            // receiver's EOF probe returns immediately instead of blocking until a read timeout".
            // The client already honours it for its request; this side did not honour it for the
            // response, and relied on `stream` dropping at the end of this function to send the
            // FIN.
            //
            // That drop is not synchronous with the client's next read. `read_framed` finishes the
            // payload and then probes one byte for EOF; if the FIN has not landed, that probe
            // blocks for the WHOLE read timeout and comes back `EAGAIN`, which the client reports
            // as `Protocol("read after frame failed: Resource temporarily unavailable")` — a
            // completed job thrown away and the timeout burned, on a correct exchange. It shows up
            // under CPU contention, which is exactly when a node is busy.
            //
            // Ignoring the error is right: the response is already written, and a peer that has
            // gone away is not this side's problem to report.
            let _ = stream.shutdown(std::net::Shutdown::Write);
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

    // ADR-0079 Decision 5: the boot line prints the DELIVERED set — what the child actually gets,
    // not what someone meant to configure — in the same line that prints the backend and the
    // ceiling. An operator who reads this line knows the posture without trusting a promise.
    let delivered = misaka_palw::host_security::worker_environment();
    let backend = confinement_backend_in_force();
    eprintln!(
        "[palw-agent] confinement backend {} | worker env ({} of {} allowlisted): {} | workdir {} | resident ceiling {} bytes ({})",
        backend.name(),
        delivered.vars.len(),
        PALW_WORKER_ENV_ALLOWLIST.len() + misaka_palw::host_security::PALW_WORKER_ENV_PINNED.len(),
        delivered.vars.keys().cloned().collect::<Vec<_>>().join(","),
        cfg.workdir.display(),
        cfg.max_resident_bytes,
        arm_memory_ceiling(cfg.max_resident_bytes).name(),
    );
    if backend == ConfinementBackend::None {
        eprintln!(
            "[palw-agent] NOTE: no platform confinement backend is in force on this host. The environment \
             discipline above still applies; ADR-0079 Decision 10 is what refuses to let such a host be a \
             PUBLIC gateway entrance."
        );
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
        let (status, _stdout, stderr) = run_captured(
            Command::new(&cfg.worker).args(["--mode", "v2-selftest"]),
            &cfg.workdir,
            SELFTEST_TIMEOUT,
            "worker v2-selftest",
        );
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

// ---------------------------------------------------------------------------------------------
// ADR-0079 Decisions 5 and 6 — the supervisor's own assertions
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(workdir: PathBuf, max_resident_bytes: u64) -> AgentConfig {
        AgentConfig {
            listen: workdir.join("agent.sock"),
            worker: PathBuf::from("/nonexistent/palw-worker"),
            job_timeout: Duration::from_secs(5),
            worst_case_job_ms: 1,
            allow_ungated: true,
            max_conns: 8,
            workdir,
            max_resident_bytes,
        }
    }

    fn test_state(cfg: AgentConfig) -> AgentState {
        AgentState {
            cfg,
            identity: WorkerIdentity {
                runtime_manifest_hash: Hash64::default(),
                model_profile_id: Hash64::default(),
                runtime_class_id: Hash64::default(),
                shape_profile_id: Hash64::default(),
                trace_scheme_id: Hash64::default(),
                cu_ruleset_id: Hash64::default(),
                tokenizer_id: Hash64::default(),
                golden_vector_root: Hash64::default(),
                golden_registered: false,
                max_context_tokens: 4096,
            },
            quarantined: false,
            selftest_passed: false,
            job_slot: Mutex::new(()),
            recent_jobs: Mutex::new((VecDeque::new(), HashSet::new())),
            conns: AtomicUsize::new(0),
            jobs_total: AtomicU64::new(0),
            jobs_ok: AtomicU64::new(0),
            jobs_rejected: AtomicU64::new(0),
            jobs_failed: AtomicU64::new(0),
            timeouts_total: AtomicU64::new(0),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("palw-agent-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// **ADR-0079 S2, through the supervisor's own spawn helper.** `run_captured` is the path the
    /// boot manifest probe and the boot selftest take; this asserts what a child launched through
    /// it ACTUALLY receives, and asserts it by EQUALITY — a "contains" test passes on a spawn that
    /// also forwards the operator's SSH agent socket.
    #[test]
    fn a_worker_child_receives_exactly_the_allowlist() {
        let Some(env_bin) = ["/usr/bin/env", "/bin/env"].iter().map(PathBuf::from).find(|p| p.is_file()) else {
            eprintln!("no /usr/bin/env on this host — skipping the delivered-set assertion");
            return;
        };
        // SAFETY: set before this test spawns anything; the harness runs tests in threads, and
        // these names are only read by this module's own environment builder.
        unsafe {
            std::env::set_var("SSH_AUTH_SOCK", "/private/tmp/must-not-be-inherited");
            std::env::set_var("MISAKA_PALW_GGUF", "/srv/models/pinned.gguf");
        }
        let workdir = scratch("env");
        let (status, stdout, stderr) = run_captured(&mut Command::new(&env_bin), &workdir, Duration::from_secs(20), "env probe");
        assert!(status.success(), "env exited {status}: {}", String::from_utf8_lossy(&stderr));

        let mut got: Vec<String> = String::from_utf8_lossy(&stdout).lines().map(str::to_string).collect();
        got.sort();
        let mut want = misaka_palw::host_security::worker_environment().as_env_lines();
        want.sort();
        assert_eq!(got, want, "the child's environment must EQUAL the delivered allowlist, not contain it");

        assert!(!got.iter().any(|l| l.starts_with("SSH_AUTH_SOCK=")), "the operator's SSH agent reached the model process");
        assert!(!got.iter().any(|l| l.starts_with("PATH=")), "PATH left the allowlist in ADR-0079 SA-4");
        for line in &got {
            let name = line.split('=').next().unwrap_or_default();
            let allowed = PALW_WORKER_ENV_ALLOWLIST.contains(&name)
                || misaka_palw::host_security::PALW_WORKER_ENV_PINNED.iter().any(|(n, _)| *n == name);
            assert!(allowed, "{name} is not on the in-tree allowlist and must not be delivered");
        }
        std::fs::remove_dir_all(&workdir).ok();
    }

    /// **ADR-0079 S9.** A child over the resident ceiling is killed — and the supervisor is still
    /// there afterwards, still supervising, still answering. A ceiling of one byte is a real
    /// overshoot for any real process, so this exercises the real measurement and the real kill
    /// rather than a mocked one.
    #[test]
    fn a_job_over_the_resident_ceiling_is_a_failed_job_and_the_supervisor_survives() {
        let (_, measure) = resident_bytes(std::process::id());
        if measure == misaka_palw::host_security::ResidentMeasure::Unavailable {
            eprintln!("no resident measurement on this platform — the ceiling reports `none` honestly and cannot bind");
            return;
        }

        let mut hog = Command::new("/bin/sh")
            .args(["-c", "sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn a child to supervise");
        let outcome = supervise_child(&mut hog, Duration::from_secs(20), 1);
        match outcome {
            SupervisionOutcome::MemoryCeiling { observed, limit, .. } => {
                assert_eq!(limit, 1);
                assert!(observed > 1, "a live process is resident for more than one byte");
            }
            other => panic!("expected the ceiling to bind, got {other:?}"),
        }

        // ...and the supervisor keeps serving. A second job runs to completion through the same
        // loop: the kill above was the JOB's end, not the node's.
        let mut next = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the next job");
        match supervise_child(&mut next, Duration::from_secs(20), u64::MAX) {
            SupervisionOutcome::Exited(status) => assert!(status.success(), "the next job must run normally"),
            other => panic!("the supervisor stopped serving after a ceiling kill: {other:?}"),
        }

        // The agent's own state: a failed job is counted, the slot is free, and health is Ready —
        // the node keeps its tip, its peers and its seat (S9's second half).
        let workdir = scratch("ceiling");
        let state = test_state(test_config(workdir.clone(), 1));
        let response = failed(&state, "memory_ceiling", "resident ceiling exceeded".into());
        assert!(matches!(response, PalwAgentResponseV1::JobFailed { .. }), "a ceiling overshoot is a JobFailed");
        assert_eq!(state.jobs_failed.load(Ordering::Relaxed), 1);
        assert_eq!(state.health().state, PalwAgentStateV1::Ready, "the supervisor is still ready to serve");
        std::fs::remove_dir_all(&workdir).ok();
    }

    /// The working directory a worker is given is neither the operator's home nor a path it
    /// inherited by accident, and it exists before the first spawn.
    #[test]
    fn the_worker_working_directory_is_explicit_and_not_the_operators_home() {
        let dir = worker_working_dir(None).expect("a workdir");
        assert!(dir.is_dir(), "{} must exist before the first spawn", dir.display());
        if let Some(home) = std::env::var_os("HOME") {
            assert_ne!(dir, PathBuf::from(home), "the worker must not run in the operator's home");
        }
    }
}
