//! `palw-reexecutor` — the ADR-0034 §6 re-executor agent, Stage 0 (shadow capability).
//!
//! Automates the §9.3 sequence: detect backend → resolve family/version → scan local
//! artifacts against binding rows → verify memory fit → run goldens → run the replay
//! benchmark → emit the ready set → derive `max_model_band` → emit the signed capability →
//! heartbeat. No human enumerates models: the operator's TOML policy narrows what this host
//! offers, and everything else is derived from records.
//!
//! Every DECISION is a pure function in this crate's library and is unit-tested there; this
//! binary only performs IO: drive the pinned `palw-worker`, hash files, read/write the state
//! directory, resolve DAA, and sign. Stage-0 discipline: capability records are written
//! locally (`capability-<nonce>.json`) — no carriage kind exists for them yet, no value
//! moves, nothing here can ground an offense.
//!
//! Modes:
//! * `keygen`     — a REEXECUTOR identity (fresh ML-DSA-87 seed; never a production
//!                  validator key — the namespaces stay disjoint by rule, like the drill's).
//! * `probe`      — drive `--mode v2-manifest`, resolve the class tag and routing keys,
//!                  resolve memory; print the host probe.
//! * `scan`       — load binding/definition records, hash held artifacts, print which
//!                  bindings are admissible and held (and why the rest are not).
//! * `qualify`    — for each held admissible binding: `v2-selftest` then `v2-replay-bench`
//!                  at the binding's credited ceiling; append qualification records.
//! * `capability` — judge readiness from the state, assemble, sign and write the capability
//!                  record (nonce reserved BEFORE use, so a crash cannot reuse one).
//! * `run`        — the whole sequence, then re-issue on the heartbeat. Continuous mode
//!                  requires a node RPC for real DAA; `--now-daa` is single-shot only —
//!                  this agent does not invent clocks.

use clap::{Parser, Subcommand};
use kaspa_addresses::Prefix;
use kaspa_consensus_core::palw_registry::PalwClassRegistrationV1;
use kaspa_consensus_core::palw_routing::{
    ModelDefinitionV1, PALW_ROUTING_MLDSA87_CAPABILITY_CONTEXT, verifier_capability_message_v1, verify_ready_binding_v1,
};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::prelude::{ConnectOptions, ConnectStrategy};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use misaka_palw_reexecutor::{
    CAPABILITY_RECORD_SCHEMA_V1, CapabilityInputsV1, CapabilityRecordV1, HostProbeV1, QualificationV1, ReadyBindingRecordV1,
    ReadyProofRecordV1, ReexecutorPolicyV1, RefusalRecordV1, artifact_matches_definition_v1, assemble_capability_v1,
    bench_decode_tokens_v1, binding_admissible_v1, binding_ready_v1, build_host_probe_v1, hex64, next_capability_nonce,
    parse_bench_summary_v1,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MANIFEST_TIMEOUT: Duration = Duration::from_secs(60);
/// After the child exits, its stdout must reach EOF promptly; a wrapper script that
/// backgrounded a grandchild holding the pipe would otherwise hang the agent past every
/// timeout. The worker contract forbids such children; this bound turns a violation into an
/// error instead of a hang.
const STDOUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "palw-reexecutor", about = "MISAKA PALW Stage-0 re-executor agent (ADR-0034 §6) — telemetry only, no value moves")]
struct Cli {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    /// Generate a REEXECUTOR ML-DSA-87 identity seed (never reuse a production validator key).
    Keygen {
        /// Seed file to write (0600).
        #[arg(long)]
        out: PathBuf,
        /// Address prefix for the printed funding address.
        #[arg(long, default_value = "testnet")]
        prefix: String,
    },
    /// Detect the backend: worker manifest → class tag → routing keys → memory.
    Probe {
        #[arg(long)]
        config: PathBuf,
    },
    /// Scan bindings, definitions and held artifacts; print admissibility per binding.
    Scan {
        #[arg(long)]
        config: PathBuf,
    },
    /// Run goldens + the replay bench for every held admissible binding; append state.
    Qualify {
        #[arg(long)]
        config: PathBuf,
        /// Re-qualify even if a record already exists for the binding.
        #[arg(long)]
        requalify: bool,
    },
    /// Assemble, sign and write the capability record from the current state.
    Capability {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        key: PathBuf,
        /// Node wRPC endpoint for the DAA anchor (host:port, borsh).
        #[arg(long)]
        rpc: Option<String>,
        /// Explicit DAA score (single-shot alternative to --rpc; never invented).
        #[arg(long)]
        now_daa: Option<u64>,
    },
    /// The whole §9.3 sequence, then heartbeat re-issuance.
    Run {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        rpc: Option<String>,
        #[arg(long)]
        now_daa: Option<u64>,
        /// One pass (probe → scan → qualify → capability), then exit.
        #[arg(long)]
        once: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.mode {
        Mode::Keygen { out, prefix } => keygen(&out, &prefix),
        Mode::Probe { config } => probe_cmd(&config),
        Mode::Scan { config } => scan_cmd(&config),
        Mode::Qualify { config, requalify } => qualify_cmd(&config, requalify),
        Mode::Capability { config, key, rpc, now_daa } => capability_cmd(&config, &key, rpc.as_deref(), now_daa),
        Mode::Run { config, key, rpc, now_daa, once } => run_cmd(&config, &key, rpc.as_deref(), now_daa, once),
    };
    if let Err(e) = result {
        eprintln!("[palw-reexecutor] FATAL: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------------------------
// Shared IO helpers
// ---------------------------------------------------------------------------------------------

fn load_policy(path: &Path) -> Result<ReexecutorPolicyV1, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read policy {}: {e}", path.display()))?;
    let policy: ReexecutorPolicyV1 = toml::from_str(&text).map_err(|e| format!("policy toml: {e}"))?;
    policy.validate()?;
    Ok(policy)
}

fn load_key(path: &Path) -> Result<ValidatorKey, String> {
    let seed = load_validator_seed(&path.display().to_string())?;
    Ok(ValidatorKey::from_seed(seed))
}

/// Drives one worker mode that prints a single JSON document to stdout (`v2-manifest`,
/// `v2-selftest`, `v2-replay-bench`). stderr is inherited (the worker narrates progress
/// there, and an un-piped stderr cannot hit the 64 KiB pipe-drain deadlock the subprocess
/// bridge in `misaka-palw` documents); a non-zero exit is an error carrying the mode name —
/// for `v2-selftest` that IS the quarantine signal.
///
/// Environment hygiene: the two worker env vars are REMOVED first and only then set from
/// explicit arguments — an ambient `MISAKA_PALW_GOLDEN` exported in the operator's shell
/// would otherwise silently flip the probed manifest identity per shell.
fn run_worker_json(worker: &str, args: &[&str], envs: &[(&str, &str)], timeout: Duration) -> Result<serde_json::Value, String> {
    let mut cmd = Command::new(worker);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    cmd.env_remove("MISAKA_PALW_GOLDEN").env_remove("MISAKA_PALW_GGUF");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().map_err(|e| format!("spawn {worker}: {e}"))?;
    let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        let _ = done_tx.send(buf);
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // The child exited; its stdout must reach EOF promptly. A bounded wait, not a
                // bare join: a grandchild holding the pipe open would otherwise hang the
                // agent past every configured timeout.
                let stdout = done_rx.recv_timeout(STDOUT_DRAIN_TIMEOUT).map_err(|_| {
                    format!("worker {args:?} exited but its stdout never closed (a backgrounded child holds the pipe?)")
                })?;
                if !status.success() {
                    return Err(format!("worker {args:?} exited with {status} — treat as refusal/quarantine, never as data"));
                }
                return serde_json::from_slice(&stdout).map_err(|e| format!("worker {args:?} printed invalid JSON: {e}"));
            }
            Ok(None) => {}
            Err(e) => return Err(format!("wait worker: {e}")),
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("worker {args:?} did not finish within {timeout:?}"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Total memory: the policy's declared value wins; otherwise detect, and a detection failure
/// is an error — "assume enough" is not a mode.
fn resolve_total_memory(policy: &ReexecutorPolicyV1) -> Result<u64, String> {
    if policy.total_memory_bytes != 0 {
        return Ok(policy.total_memory_bytes);
    }
    detect_total_memory()
        .ok_or_else(|| "could not detect total memory on this platform — set total_memory_bytes in the policy explicitly".to_string())
}

fn detect_total_memory() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kib: u64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
                return kib.checked_mul(1024);
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("sysctl").args(["-n", "hw.memsize"]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout).trim().parse().ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Probe WITH the golden set registered: the worker's certified identity
/// (`runtime_manifest_hash_v2`) folds the golden root in, fleet bindings pin that
/// golden-populated hash, and a bare probe would report the unpopulated sentinel — refusing
/// every real binding with a misleading "different runtime manifest" error.
fn host_probe(policy: &ReexecutorPolicyV1, validated_rows: &[PalwClassRegistrationV1]) -> Result<HostProbeV1, String> {
    let envs = [("MISAKA_PALW_GOLDEN", policy.golden_set.as_str())];
    let manifest = run_worker_json(&policy.worker_bin, &["--mode", "v2-manifest"], &envs, MANIFEST_TIMEOUT)?;
    build_host_probe_v1(&manifest, resolve_total_memory(policy)?, validated_rows)
}

/// The persisted digest memo: `(size, mtime)` → sha256, so an unchanged 1.2 GB artifact is
/// hashed once, not once per heartbeat (~345 GB/day of pointless reads at a 300 s cadence).
/// The memo lives in the operator's own state dir and only ever short-circuits work on a
/// file whose size AND mtime are unchanged; a same-size mtime-restored swap evades it, which
/// is acceptable here — the artifact is the operator's own file, and a wrong artifact still
/// fails at golden/replay time. `qualify --requalify` paths re-hash by clearing the memo.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct DigestMemo {
    entries: HashMap<String, DigestMemoEntry>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct DigestMemoEntry {
    size: u64,
    mtime_unix: u64,
    sha256_hex: String,
}

fn digest_memo_path(policy: &ReexecutorPolicyV1) -> PathBuf {
    Path::new(&policy.state_dir).join("artifact-digests.json")
}

fn load_digest_memo(policy: &ReexecutorPolicyV1) -> DigestMemo {
    std::fs::read_to_string(digest_memo_path(policy)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn store_digest_memo(policy: &ReexecutorPolicyV1, memo: &DigestMemo) {
    // Best-effort: a lost memo only costs a re-hash, never correctness.
    if std::fs::create_dir_all(&policy.state_dir).is_ok() {
        let _ = std::fs::write(digest_memo_path(policy), serde_json::to_string(memo).expect("serializable"));
    }
}

fn file_identity(path: &Path) -> Result<(u64, u64), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("stat {}: {e}", path.display()))?;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    Ok((meta.len(), mtime))
}

fn sha256_file_memoized(path: &Path, memo: &mut DigestMemo) -> Result<(u64, [u8; 32]), String> {
    let (size, mtime_unix) = file_identity(path)?;
    let key = path.display().to_string();
    if let Some(entry) = memo.entries.get(&key)
        && entry.size == size
        && entry.mtime_unix == mtime_unix
        && entry.sha256_hex.len() == 64
    {
        let mut cached = [0u8; 32];
        if faster_hex::hex_decode(entry.sha256_hex.as_bytes(), &mut cached).is_ok() {
            return Ok((size, cached));
        }
    }
    let (hashed_size, digest) = sha256_file(path)?;
    memo.entries.insert(key, DigestMemoEntry { size: hashed_size, mtime_unix, sha256_hex: faster_hex::hex_string(&digest) });
    Ok((hashed_size, digest))
}

fn sha256_file(path: &Path) -> Result<(u64, [u8; 32]), String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let total = std::io::copy(&mut file, &mut hasher).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok((total, hasher.finalize().into()))
}

fn load_borsh_files<T: borsh::BorshDeserialize>(paths: &[String], what: &str) -> Result<Vec<(String, T)>, String> {
    let mut out = Vec::new();
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| format!("read {what} {path}: {e}"))?;
        let value = borsh::from_slice::<T>(&bytes).map_err(|e| format!("decode {what} {path}: {e}"))?;
        out.push((path.clone(), value));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The scan: records joined, artifacts hashed, admissibility judged
// ---------------------------------------------------------------------------------------------

struct ScanOutcome {
    probe: HostProbeV1,
    /// Admissible AND held: (binding, definition, artifact path).
    eligible: Vec<(PalwClassRegistrationV1, ModelDefinitionV1, String)>,
    /// Everything refused, with the reason (for the operator, and for the JSON report).
    refused: Vec<RefusalRecordV1>,
}

fn scan(policy: &ReexecutorPolicyV1) -> Result<ScanOutcome, String> {
    let (blockrate, block_ms) = policy.blockrate()?;
    let bindings: Vec<(String, PalwClassRegistrationV1)> = load_borsh_files(&policy.binding_paths, "binding row")?;
    let definitions: Vec<(String, ModelDefinitionV1)> = load_borsh_files(&policy.definition_paths, "model definition")?;
    let mut refused: Vec<RefusalRecordV1> = Vec::new();

    // The registry-store precondition, applied per identity rather than all-or-nothing: rows
    // sharing a registration or class id are ALL excluded (qualifying against one row and
    // being judged against another is the split-key hazard), but an unrelated healthy row —
    // and the running heartbeat that depends on it — survives a bad drop-in.
    let mut registration_counts: HashMap<Hash64, u32> = HashMap::new();
    let mut class_counts: HashMap<Hash64, u32> = HashMap::new();
    for (_, binding) in &bindings {
        *registration_counts.entry(binding.registration_id()).or_insert(0) += 1;
        *class_counts.entry(binding.runtime_class_id).or_insert(0) += 1;
    }
    let (coherent, incoherent): (Vec<_>, Vec<_>) = bindings
        .into_iter()
        .partition(|(_, b)| registration_counts[&b.registration_id()] == 1 && class_counts[&b.runtime_class_id] == 1);
    for (source, binding) in incoherent {
        refused.push(RefusalRecordV1 {
            binding_id: hex64(&binding.registration_id()),
            reason: format!(
                "row set incoherent: another row shares its registration or runtime_class_id (from {source}) — all rows of \
                 that identity are excluded (the split-key hazard: qualify against one row, be judged against another)"
            ),
        });
    }
    // Same rule for definitions: two records claiming one model_profile_id could make the
    // artifact join and the binding join disagree about which artifact "the" model is.
    let mut profile_counts: HashMap<Hash64, u32> = HashMap::new();
    for (_, def) in &definitions {
        *profile_counts.entry(def.model_profile_id).or_insert(0) += 1;
    }
    let definitions: Vec<(String, ModelDefinitionV1)> =
        definitions.into_iter().filter(|(_, d)| profile_counts[&d.model_profile_id] == 1).collect();

    // The validated rows double as the second tag witness for backend naming.
    let validated_rows: Vec<PalwClassRegistrationV1> =
        coherent.iter().filter(|(_, b)| b.validate(&blockrate, block_ms).is_ok()).map(|(_, b)| b.clone()).collect();
    let probe = host_probe(policy, &validated_rows)?;

    // Hash held artifacts once per CHANGE, not once per call: the digest memo persists in
    // the state dir so heartbeats and back-to-back CLI commands do not re-read gigabytes.
    let mut memo = load_digest_memo(policy);
    let mut held: HashMap<Hash64, String> = HashMap::new(); // model_profile_id → artifact path
    for path in &policy.model_paths {
        let (size, digest) = sha256_file_memoized(Path::new(path), &mut memo)?;
        for (_, def) in &definitions {
            if artifact_matches_definition_v1(size, &digest, def) {
                held.entry(def.model_profile_id).or_insert_with(|| path.clone());
            }
        }
    }
    store_digest_memo(policy, &memo);

    let mut eligible = Vec::new();
    for (source, binding) in coherent {
        let id_hex = hex64(&binding.registration_id());
        let Some((_, definition)) = definitions.iter().find(|(_, d)| d.model_profile_id == binding.model_profile_id) else {
            refused.push(RefusalRecordV1 {
                binding_id: id_hex,
                reason: format!("no (unique) model definition for its model_profile_id (from {source})"),
            });
            continue;
        };
        if let Err(reason) = binding_admissible_v1(&binding, definition, &probe, policy, &blockrate, block_ms) {
            refused.push(RefusalRecordV1 { binding_id: id_hex, reason });
            continue;
        }
        let Some(artifact) = held.get(&binding.model_profile_id) else {
            refused.push(RefusalRecordV1 {
                binding_id: id_hex,
                reason: "admissible, but the artifact is not held (no local file matches the signed digest)".into(),
            });
            continue;
        };
        eligible.push((binding, definition.clone(), artifact.clone()));
    }
    Ok(ScanOutcome { probe, eligible, refused })
}

fn scan_report_json(outcome: &ScanOutcome) -> serde_json::Value {
    serde_json::json!({
        "schema": "misaka.palw-reexecutor.scan.v1",
        "class_tag": outcome.probe.class_tag,
        "execution_family": format!("{:?}", outcome.probe.execution_family),
        "family_version": outcome.probe.family_version,
        "total_memory_bytes": outcome.probe.total_memory_bytes,
        "eligible": outcome.eligible.iter().map(|(b, _, artifact)| serde_json::json!({
            "binding_id": hex64(&b.registration_id()),
            "model_band": format!("{:?}", b.model_band),
            "credited_ceiling_tokens": b.credited_ceiling_tokens,
            "artifact": artifact,
        })).collect::<Vec<_>>(),
        "refused": outcome.refused,
    })
}

// ---------------------------------------------------------------------------------------------
// State: qualifications and the nonce
// ---------------------------------------------------------------------------------------------

fn qualifications_path(policy: &ReexecutorPolicyV1) -> PathBuf {
    Path::new(&policy.state_dir).join("qualifications.jsonl")
}

fn nonce_path(policy: &ReexecutorPolicyV1) -> PathBuf {
    Path::new(&policy.state_dir).join("capability.nonce")
}

/// Latest qualification per binding id (append-only log; the last line wins).
/// Latest qualification per binding id. Torn lines are WARNED and skipped, not fatal: this
/// is an append-only recovery log, a kill mid-append must not brick every later mode, and
/// skipping a broken record only causes a re-qualification — the safe direction.
fn read_qualifications(policy: &ReexecutorPolicyV1) -> Result<HashMap<String, QualificationV1>, String> {
    let path = qualifications_path(policy);
    let mut out = HashMap::new();
    if !path.exists() {
        return Ok(out);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<QualificationV1>(line) {
            Ok(q) => {
                out.insert(q.binding_id_hex.clone(), q);
            }
            Err(e) => eprintln!(
                "[palw-reexecutor] WARN: qualifications line {} is unreadable ({e}) — skipping it; the binding will re-qualify",
                i + 1
            ),
        }
    }
    Ok(out)
}

/// One write syscall per record (the line and its newline together): a kill between two
/// writes must not leave a half-line the next append would merge into.
fn append_qualification(policy: &ReexecutorPolicyV1, q: &QualificationV1) -> Result<(), String> {
    std::fs::create_dir_all(&policy.state_dir).map_err(|e| format!("mkdir {}: {e}", policy.state_dir))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(qualifications_path(policy))
        .map_err(|e| format!("open qualifications: {e}"))?;
    let line = format!("{}\n", serde_json::to_string(q).expect("serializable"));
    f.write_all(line.as_bytes()).map_err(|e| format!("append qualification: {e}"))?;
    Ok(())
}

/// Reserve the next nonce: read, increment, PERSIST, then hand it out — a crash between the
/// persist and the emission wastes a nonce, never reuses one. Two mechanisms carry that
/// guarantee all the way down: an exclusive lockfile serializes concurrent processes (a
/// heartbeat daemon plus a manual `capability` would otherwise both read N and both sign
/// N+1 — equal nonces that `supersedes()` cannot order, which drops the verifier from every
/// panel), and a write-temp/fsync/rename makes the counter update atomic (a torn in-place
/// write would silently REGRESS the counter and reuse every nonce above it).
fn reserve_nonce(policy: &ReexecutorPolicyV1) -> Result<u64, String> {
    std::fs::create_dir_all(&policy.state_dir).map_err(|e| format!("mkdir {}: {e}", policy.state_dir))?;
    let path = nonce_path(policy);
    let lock_path = path.with_extension("lock");
    let _lock = NonceLock::acquire(&lock_path)?;
    let previous = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text.trim().parse::<u64>().map_err(|e| {
            format!(
                "corrupt nonce file {}: {e} — refusing to guess; restore the last issued nonce by hand (never a smaller one)",
                path.display()
            )
        })?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("read nonce: {e}")),
    };
    let next = next_capability_nonce(previous)?;
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        f.write_all(next.to_string().as_bytes()).map_err(|e| format!("write nonce tmp: {e}"))?;
        f.sync_all().map_err(|e| format!("fsync nonce tmp: {e}"))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename nonce into place: {e}"))?;
    Ok(next)
}

/// Exclusive-creation lockfile guarding the nonce read-modify-write. Held for the few
/// milliseconds of a counter update; removed on drop. If a crash strands it, the next
/// issuance fails with an explicit message rather than guessing — fail-stop, never a
/// silently shared nonce.
struct NonceLock(PathBuf);

impl NonceLock {
    fn acquire(path: &Path) -> Result<Self, String> {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(_) => Ok(Self(path.to_path_buf())),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
                "another issuance holds the nonce lock at {} — if no other palw-reexecutor process is running, a crash \
                 stranded it; remove the lockfile and retry",
                path.display()
            )),
            Err(e) => Err(format!("create nonce lock {}: {e}", path.display())),
        }
    }
}

impl Drop for NonceLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Qualification (goldens + bench per eligible binding)
// ---------------------------------------------------------------------------------------------

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Qualifies one binding, recording exactly which STAGE failed: a bench timeout on a healthy
/// runtime must never masquerade as a golden quarantine (`selftest_passed` stays true, the
/// bench is `None`, and the reason survives into the state log instead of dying on stderr).
/// `selftest_already_passed` lets one selftest pass cover every binding over the same
/// (worker, golden, artifact) — its inputs are only those three.
fn qualify_one(
    policy: &ReexecutorPolicyV1,
    binding: &PalwClassRegistrationV1,
    artifact: &str,
    selftest_already_passed: bool,
) -> QualificationV1 {
    let envs = [("MISAKA_PALW_GOLDEN", policy.golden_set.as_str()), ("MISAKA_PALW_GGUF", artifact)];
    let id_hex = hex64(&binding.registration_id());
    if !selftest_already_passed {
        eprintln!("[palw-reexecutor] qualify {}…: v2-selftest (goldens)", &id_hex[..16]);
        // A non-zero selftest exit is the quarantine signal — recorded, not retried silently.
        if let Err(e) =
            run_worker_json(&policy.worker_bin, &["--mode", "v2-selftest"], &envs, Duration::from_secs(policy.selftest_timeout_secs))
        {
            eprintln!("[palw-reexecutor] qualify {}…: SELFTEST FAILED — {e}", &id_hex[..16]);
            return QualificationV1 {
                binding_id_hex: id_hex,
                selftest_passed: false,
                bench: None,
                failure_reason: Some(format!("selftest refused: {e}")),
                qualified_unix: now_unix(),
            };
        }
    }
    // The decode override must fit the worker's own `prefill + decode ≤ context` rule, or a
    // format-bound ceiling (4095) over a 12-prefill golden dies on every run.
    let decode =
        bench_decode_tokens_v1(binding.credited_ceiling_tokens, policy.bench_golden_prefill_tokens, policy.bench_context_tokens);
    eprintln!(
        "[palw-reexecutor] qualify {}…: v2-replay-bench ({} runs, decode {decode} of ceiling {})",
        &id_hex[..16],
        policy.bench_runs,
        binding.credited_ceiling_tokens
    );
    let runs = policy.bench_runs.to_string();
    let decode_arg = decode.to_string();
    let bench_result = run_worker_json(
        &policy.worker_bin,
        &["--mode", "v2-replay-bench", "--name", &policy.bench_golden_name, "--runs", &runs, "--decode", &decode_arg],
        &envs,
        Duration::from_secs(policy.bench_timeout_secs),
    )
    .and_then(|doc| parse_bench_summary_v1(&doc));
    match bench_result {
        Ok(bench) => QualificationV1 {
            binding_id_hex: id_hex,
            selftest_passed: true,
            bench: Some(bench),
            failure_reason: None,
            qualified_unix: now_unix(),
        },
        Err(e) => {
            eprintln!("[palw-reexecutor] qualify {}…: BENCH FAILED (goldens passed) — {e}", &id_hex[..16]);
            QualificationV1 {
                binding_id_hex: id_hex,
                selftest_passed: true,
                bench: None,
                failure_reason: Some(format!("bench failed after a passing selftest: {e}")),
                qualified_unix: now_unix(),
            }
        }
    }
}

fn qualify_all(policy: &ReexecutorPolicyV1, outcome: &ScanOutcome, requalify: bool) -> Result<(), String> {
    let existing = read_qualifications(policy)?;
    // One selftest pass covers every binding over the same artifact within this run — its
    // inputs are (worker, golden, artifact) only; the bench stays per-binding (the decode
    // ceiling genuinely differs).
    let mut selftest_passed_artifacts: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (binding, _, artifact) in &outcome.eligible {
        let id_hex = hex64(&binding.registration_id());
        if !requalify && existing.contains_key(&id_hex) {
            eprintln!("[palw-reexecutor] qualify {}…: already recorded (use --requalify to redo)", &id_hex[..16]);
            continue;
        }
        let q = qualify_one(policy, binding, artifact, selftest_passed_artifacts.contains(artifact));
        if q.selftest_passed {
            selftest_passed_artifacts.insert(artifact.clone());
        }
        append_qualification(policy, &q)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// DAA resolution
// ---------------------------------------------------------------------------------------------

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime")
}

async fn connect(node_rpc: &str) -> Result<KaspaRpcClient, String> {
    let url = format!("ws://{node_rpc}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None).map_err(|e| format!("wRPC client: {e}"))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(5_000)),
        strategy: ConnectStrategy::Retry,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| format!("connect {url}: {e}"))?;
    Ok(client)
}

fn resolve_now_daa(rpc: Option<&str>, now_daa: Option<u64>) -> Result<u64, String> {
    match (rpc, now_daa) {
        (Some(_), Some(_)) => Err(
            "both --rpc and --now-daa given — ambiguous DAA source; pass exactly one, silently preferring either would hide a mistake"
                .into(),
        ),
        (Some(endpoint), None) => rt().block_on(async {
            let client = connect(endpoint).await?;
            let dag = client.get_block_dag_info().await.map_err(|e| format!("getBlockDagInfo: {e}"))?;
            Ok(dag.virtual_daa_score)
        }),
        (None, Some(daa)) => Ok(daa),
        (None, None) => Err("no DAA source: pass --rpc (preferred) or --now-daa — this agent does not invent clocks".into()),
    }
}

// ---------------------------------------------------------------------------------------------
// Capability emission — judgment first (pure, no side effects), then the nonce-consuming emit
// ---------------------------------------------------------------------------------------------

/// Judges readiness from the state log. Separate from the emit so the run loop can see an
/// empty ready set and SKIP the emission (heartbeating its non-readiness) without consuming
/// a nonce or dying — a transient qualification failure must degrade the offer, never
/// crash-loop the daemon.
fn judge_readiness(policy: &ReexecutorPolicyV1, outcome: &ScanOutcome) -> Result<(Vec<Hash64>, Vec<RefusalRecordV1>), String> {
    let (_, block_ms) = policy.blockrate()?;
    let qualifications = read_qualifications(policy)?;
    let mut ready_ids = Vec::new();
    let mut not_ready = Vec::new();
    for (binding, _, _) in &outcome.eligible {
        let id = binding.registration_id();
        let id_hex = hex64(&id);
        match qualifications.get(&id_hex) {
            Some(q) => match binding_ready_v1(q, binding, policy, block_ms) {
                Ok(()) => ready_ids.push(id),
                Err(reason) => not_ready.push(RefusalRecordV1 { binding_id: id_hex, reason }),
            },
            None => not_ready.push(RefusalRecordV1 { binding_id: id_hex, reason: "not qualified yet (run qualify)".into() }),
        }
    }
    for refusal in &not_ready {
        eprintln!("[palw-reexecutor] not ready {}…: {}", &refusal.binding_id[..16], refusal.reason);
    }
    Ok((ready_ids, not_ready))
}

fn emit_capability(
    policy: &ReexecutorPolicyV1,
    key: &ValidatorKey,
    outcome: &ScanOutcome,
    ready_ids: Vec<Hash64>,
    not_ready: Vec<RefusalRecordV1>,
    now_daa: u64,
) -> Result<CapabilityRecordV1, String> {
    if ready_ids.is_empty() {
        // Refuse BEFORE reserving a nonce: an unemittable offer must not burn counter space.
        return Err("the ready set is empty — nothing to offer; not emitting a capability".into());
    }
    let nonce = reserve_nonce(policy)?; // reserved BEFORE use — a crash wastes it, never reuses it
    let assembled = assemble_capability_v1(CapabilityInputsV1 {
        verifier_id: key.validator_id,
        probe: &outcome.probe,
        policy,
        ready_binding_ids: ready_ids,
        now_daa,
        nonce,
    })?;
    let mut capability = assembled.capability;
    let message = verifier_capability_message_v1(policy.network_id.as_bytes(), &capability);
    capability.signature = key.sign_with_context(message.as_bytes().as_slice(), PALW_ROUTING_MLDSA87_CAPABILITY_CONTEXT).to_vec();
    capability.validate().map_err(|e| format!("assembled capability does not validate (bug): {e}"))?;
    for (id, proof) in &assembled.proofs {
        if !verify_ready_binding_v1(&capability.ready_binding_root, id, proof) {
            return Err("a ready proof does not verify against the committed root (bug)".into());
        }
    }

    let capability_bytes = borsh::to_vec(&capability).expect("borsh capability");
    let record = CapabilityRecordV1 {
        schema: CAPABILITY_RECORD_SCHEMA_V1.to_owned(),
        network_id: policy.network_id.clone(),
        capability_id: hex64(&capability.capability_id()),
        verifier_id: hex64(&capability.verifier_id),
        // Without the raw verification key no third party could EVER check the signature —
        // verifier_id is only a hash of it, and re-executor identities are never bonded.
        verifier_public_key: faster_hex::hex_string(key.public_key()),
        class_tag: outcome.probe.class_tag.clone(),
        execution_family: format!("{:?}", capability.execution_family),
        family_version: capability.family_version,
        max_model_band: format!("{:?}", capability.max_model_band),
        ready_binding_root: hex64(&capability.ready_binding_root),
        max_concurrency: capability.max_concurrency,
        available_slots: capability.available_slots,
        max_accepted_replay_secs: capability.max_accepted_replay_secs,
        minimum_reward: capability.minimum_reward,
        available_bond: capability.available_bond,
        availability_expiry_daa: capability.availability_expiry_daa,
        issued_now_daa: now_daa,
        capability_nonce: capability.capability_nonce,
        signing_message: faster_hex::hex_string(message.as_bytes().as_slice()),
        capability_borsh_hex: faster_hex::hex_string(&capability_bytes),
        ready_bindings: assembled
            .proofs
            .iter()
            .map(|(id, proof)| ReadyBindingRecordV1 { binding_id: hex64(id), proof: ReadyProofRecordV1::from_proof(proof) })
            .collect(),
        not_ready,
    };

    let dir = Path::new(&policy.state_dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let pretty = serde_json::to_string_pretty(&record).expect("serializable");
    std::fs::write(dir.join(format!("capability-{nonce}.json")), &pretty).map_err(|e| format!("write capability: {e}"))?;
    std::fs::write(dir.join("capability-latest.json"), &pretty).map_err(|e| format!("write capability-latest: {e}"))?;
    eprintln!(
        "[palw-reexecutor] capability nonce {nonce} issued: {} ready binding(s), expires at daa {}",
        record.ready_bindings.len(),
        capability.availability_expiry_daa
    );
    Ok(record)
}

// ---------------------------------------------------------------------------------------------
// Modes
// ---------------------------------------------------------------------------------------------

fn keygen(out: &Path, prefix: &str) -> Result<(), String> {
    use rand::RngCore;
    let prefix = match prefix {
        "mainnet" => Prefix::Mainnet,
        "testnet" => Prefix::Testnet,
        "simnet" => Prefix::Simnet,
        "devnet" => Prefix::Devnet,
        other => return Err(format!("unknown prefix {other:?}")),
    };
    let mut seed = [0u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN];
    rand::thread_rng().fill_bytes(&mut seed);
    // The shared hardened writer: O_EXCL (no clobber, no symlink), 0600 at creation (no
    // world-readable window), fsync — the canonical keygen's discipline, inherited instead
    // of re-copied weakly.
    kaspa_pq_validator_core::write_validator_seed(&out.display().to_string(), &seed)?;
    let key = ValidatorKey::from_seed(seed);
    seed.fill(0);
    std::hint::black_box(&seed);
    println!("re-executor identity written to {}", out.display());
    println!("verifier_id      = {}", hex64(&key.validator_id));
    println!("funding address  = {}", key.funding_address(prefix));
    println!("NOTE: a RE-EXECUTOR identity. Never point this tool at a production validator seed.");
    Ok(())
}

fn probe_cmd(config: &Path) -> Result<(), String> {
    let policy = load_policy(config)?;
    // The diagnostic probe loads rows if it can (they are the second tag witness), but a
    // broken row file must not hide the probe from the operator debugging exactly that.
    let (blockrate, block_ms) = policy.blockrate()?;
    let validated_rows: Vec<PalwClassRegistrationV1> =
        match load_borsh_files::<PalwClassRegistrationV1>(&policy.binding_paths, "binding row") {
            Ok(rows) => rows.into_iter().filter(|(_, b)| b.validate(&blockrate, block_ms).is_ok()).map(|(_, b)| b).collect(),
            Err(e) => {
                eprintln!("[palw-reexecutor] WARN: binding rows unreadable ({e}) — probing with the tag ledger alone");
                Vec::new()
            }
        };
    let probe = host_probe(&policy, &validated_rows)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": "misaka.palw-reexecutor.probe.v1",
            "runtime_class_id": hex64(&probe.runtime_class_id),
            "runtime_manifest_hash": hex64(&probe.runtime_manifest_hash),
            "model_profile_id": hex64(&probe.model_profile_id),
            "class_tag": probe.class_tag,
            "execution_family": format!("{:?}", probe.execution_family),
            "family_version": probe.family_version,
            "total_memory_bytes": probe.total_memory_bytes,
        }))
        .expect("serializable")
    );
    Ok(())
}

fn scan_cmd(config: &Path) -> Result<(), String> {
    let policy = load_policy(config)?;
    let outcome = scan(&policy)?;
    println!("{}", serde_json::to_string_pretty(&scan_report_json(&outcome)).expect("serializable"));
    Ok(())
}

fn qualify_cmd(config: &Path, requalify: bool) -> Result<(), String> {
    let policy = load_policy(config)?;
    let outcome = scan(&policy)?;
    qualify_all(&policy, &outcome, requalify)?;
    println!("{}", serde_json::to_string_pretty(&scan_report_json(&outcome)).expect("serializable"));
    Ok(())
}

fn capability_cmd(config: &Path, key_path: &Path, rpc: Option<&str>, now_daa: Option<u64>) -> Result<(), String> {
    let policy = load_policy(config)?;
    let key = load_key(key_path)?;
    let outcome = scan(&policy)?;
    let (ready_ids, not_ready) = judge_readiness(&policy, &outcome)?;
    let daa = resolve_now_daa(rpc, now_daa)?;
    let record = emit_capability(&policy, &key, &outcome, ready_ids, not_ready, daa)?;
    println!("{}", serde_json::to_string_pretty(&record).expect("serializable"));
    Ok(())
}

/// One full §9.3 pass. Errors are returned, not fatal — the caller decides whether they end
/// a single-shot run or merely skip one heartbeat.
fn run_once(policy: &ReexecutorPolicyV1, key: &ValidatorKey, rpc: Option<&str>, now_daa: Option<u64>) -> Result<bool, String> {
    let outcome = scan(policy)?;
    qualify_all(policy, &outcome, false)?;
    let (ready_ids, not_ready) = judge_readiness(policy, &outcome)?;
    if ready_ids.is_empty() {
        eprintln!(
            "[palw-reexecutor] nothing ready ({} eligible, {} refusals) — heartbeating non-readiness, no capability emitted",
            outcome.eligible.len(),
            not_ready.len()
        );
        return Ok(false);
    }
    let daa = resolve_now_daa(rpc, now_daa)?;
    emit_capability(policy, key, &outcome, ready_ids, not_ready, daa)?;
    Ok(true)
}

fn run_cmd(config: &Path, key_path: &Path, rpc: Option<&str>, now_daa: Option<u64>, once: bool) -> Result<(), String> {
    let policy = load_policy(config)?;
    let key = load_key(key_path)?;
    if !once && rpc.is_none() {
        return Err(
            "continuous run needs --rpc for real DAA; --now-daa is single-shot only (--once) — this agent does not invent clocks"
                .into(),
        );
    }
    loop {
        // A transient fault (node blip, worker OOM, torn state) degrades ONE beat, never the
        // daemon: crash-looping under a supervisor would take the host off the market until
        // a human intervened — strictly worse than the fault itself. A single-shot run still
        // reports its failure as a failure.
        match run_once(&policy, &key, rpc, now_daa) {
            Ok(_) => {}
            Err(e) => {
                if once {
                    return Err(e);
                }
                eprintln!("[palw-reexecutor] WARN: this pass failed ({e}) — holding until the next heartbeat");
            }
        }
        if once {
            return Ok(());
        }
        eprintln!("[palw-reexecutor] heartbeat: sleeping {}s", policy.heartbeat_secs);
        std::thread::sleep(Duration::from_secs(policy.heartbeat_secs));
    }
}
