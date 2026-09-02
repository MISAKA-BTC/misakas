//! **Tree-level guards for ADR-0079 S8 and SA-7** — the two properties that cannot be held by a
//! unit test inside one module, because what they forbid is a shape appearing ANYWHERE.
//!
//! * **S8 / Decision 9 — artifact identity is never derived from metadata.** The audit finding
//!   this replaces is on record: a `(path, size, mtime)`-keyed digest cache in the process working
//!   directory let anyone who could write that file pass any same-sized model — on the consensus
//!   PoW path, where the consequence is a node that silently forks itself. Decision 9 promotes
//!   that from a fixed bug to a standing rule: **size, mtime, filename, a sidecar `.json`, or a
//!   previous run's answer are all the same defect.** The full read IS the check.
//! * **SA-7 — nothing logs a prompt.** Gateway, supervisor, worker and seat log token counts and
//!   roots; they log no prompt text and no prompt ids. "Private unless disputed" is false if the
//!   default log is a disclosure.
//!
//! These read the tree's own source. That is deliberate: a guard that only inspected this crate
//! would pass on the day the defect is reintroduced two crates over, which is exactly how the
//! original one arrived.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // <root>/misaka-palw/ -> <root>
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("misaka-palw has a parent").to_path_buf()
}

/// The PALW execution path: every crate that loads an artifact, spawns a worker, or serves a
/// prompt. Adding a crate to this list is how a new one comes under the guard.
const SCANNED: &[&str] = &[
    "misaka-palw/src",
    "misaka-palw-agent/src",
    "misaka-palw-gateway/src",
    "misaka-palw-worker/src",
    "misaka-palw-sdk/src",
    "misaka-palw-base0/src",
    "kaspa-pq-validator-core/src",
    "kaspa-pq-signer/src",
];

/// Only these files under [`SCANNED`] are also scanned from `kaspad`, because `kaspad/src` is the
/// whole node and this guard is about the PALW path.
const SCANNED_KASPAD_FILES: &[&str] =
    &["kaspad/src/palw_backends.rs", "kaspad/src/palw_panel.rs", "kaspad/src/palw_producer.rs", "kaspad/src/compute.rs"];

fn rust_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = repo_root();
    let mut paths = Vec::new();
    for dir in SCANNED {
        walk(&root.join(dir), &mut paths);
    }
    for file in SCANNED_KASPAD_FILES {
        let path = root.join(file);
        if path.is_file() {
            paths.push(path);
        }
    }
    assert!(paths.len() > 20, "the scan found only {} files — the guard is not looking at the tree", paths.len());
    paths.into_iter().filter_map(|p| std::fs::read_to_string(&p).ok().map(|s| (p, s))).collect()
}

fn relative(path: &Path) -> String {
    path.strip_prefix(repo_root()).unwrap_or(path).display().to_string()
}

/// **S8, the name half.** A cache of a digest keyed by anything is a cache whose key is not the
/// bytes. These names are banned outright; a legitimate need for one of them is a review, which is
/// what this test makes it.
#[test]
fn no_digest_cache_by_name() {
    const BANNED: &[&str] = &[
        "digest_cache",
        "hash_cache",
        "sha_cache",
        "sha256_cache",
        "mtime_cache",
        "artifact_digest_cache",
        "artifact_root_cache",
        "model_hash_cache",
        "gguf_cache",
        "cached_digest",
        "cached_sha",
        "cached_artifact_root",
    ];
    let mut findings = Vec::new();
    for (path, source) in rust_sources() {
        let lowered = source.to_lowercase();
        for banned in BANNED {
            if lowered.contains(banned) {
                findings.push(format!("{}: `{banned}`", relative(&path)));
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0079 Decision 9: artifact identity is never derived from metadata, and a digest cache is a key that is \
         not the bytes. Found:\n  {}",
        findings.join("\n  ")
    );
}

/// **S8, the shape half.** A struct that carries a path AND a size AND a modification time is the
/// exact key the original defect used. This test does not ban the shape — one use of it is
/// explicitly sanctioned by ADR-0077 SA-6 — it requires every use to be NAMED here, so
/// reintroducing the defect is a test failure that a reviewer must sign off with a citation.
#[test]
fn every_path_size_mtime_key_is_named_and_justified() {
    /// The allowlist, with the citation that justifies each entry.
    ///
    /// * `HeldArtifactKey` (`kaspad/src/palw_backends.rs`) — ADR-0077 SA-6: *"Mapping once is the
    ///   point; the artifact is opened read-only, its digest verified at map time, RE-VERIFIED
    ///   when the file's identity (device, inode, size) changes."* This is a PROCESS-LIFETIME
    ///   holdings map, not a persisted cache: the digest it keys was computed from the bytes by
    ///   this process, in this run, and a file whose identity changes is mapped and hashed afresh
    ///   (`a_replaced_artifact_is_mapped_and_hashed_afresh` is its own test). The residual is
    ///   stated: an in-place overwrite that preserves size and restores mtime is not detected
    ///   until the process restarts — SA-6 accepts that residual and re-verification on identity
    ///   change is the bound it chose.
    const ALLOWED: &[&str] = &["HeldArtifactKey"];

    let mut findings = Vec::new();
    for (path, source) in rust_sources() {
        for chunk in source.split("struct ").skip(1) {
            let name: String = chunk.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if name.is_empty() {
                continue;
            }
            // The declaration body: up to the first `}` at the start of a line, which is where a
            // struct declaration ends in this tree's formatting.
            let body = chunk.split("\n}").next().unwrap_or(chunk);
            let has_path = body.contains("path:") || body.contains("file:");
            let has_size = body.contains("len:") || body.contains("size:") || body.contains("bytes:");
            let has_time = body.contains("modified:") || body.contains("mtime:") || body.contains("timestamp:");
            if has_path && has_size && has_time && !ALLOWED.contains(&name.as_str()) {
                findings.push(format!("{}: struct `{name}` keys a path by size and time", relative(&path)));
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0079 Decision 9 / S8: a (path, size, mtime) key is the original defect's key. If a new one is \
         genuinely the ADR-0077 SA-6 shape (verify at map time, re-verify on identity change, never persisted), \
         add it to ALLOWED with its citation. Found:\n  {}",
        findings.join("\n  ")
    );
}

/// **S8, the persistence half.** The original defect wrote its cache to a file in the process
/// working directory. Nothing in the PALW path may write or read a digest sidecar.
#[test]
fn no_digest_is_persisted_beside_an_artifact() {
    let mut findings = Vec::new();
    for (path, source) in rust_sources() {
        for (n, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
                continue;
            }
            for sidecar in [".sha256\"", ".digest\"", ".gguf.json\"", ".artifact-root\""] {
                if line.contains(sidecar) {
                    findings.push(format!("{}:{}: writes or reads a digest sidecar {sidecar}", relative(&path), n + 1));
                }
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0079 Decision 9: a sidecar `.json` is the same defect as an mtime key — a previous run's answer is \
         not this run's check. Found:\n  {}",
        findings.join("\n  ")
    );
}

/// **S8's positive half.** The gate that must never be cheapened is still the full read: every job
/// process recomputes SHA-256 over the WHOLE artifact and compares it to the pin.
#[test]
fn the_pinned_model_gate_still_reads_every_byte() {
    let source = std::fs::read_to_string(repo_root().join("misaka-palw-worker/src/main.rs"))
        .expect("misaka-palw-worker/src/main.rs is in the tree");
    let gate = source.split("fn pinned_model_path_v2").nth(1).expect("pinned_model_path_v2 still exists");
    let body = gate.split("\n}").next().unwrap_or(gate);
    assert!(body.contains("Sha256::new()"), "the gate must hash");
    assert!(
        body.contains("std::io::copy(&mut file, &mut hasher)"),
        "the gate must stream the WHOLE file into the hasher — a length check alone is the defect Decision 9 names"
    );
    assert!(body.contains("GGUF_SHA256"), "the computed digest must be compared against the pin");
    // The size check is fine and stays: it is a fast refusal BEFORE the read, not the identity.
    // What would be the defect is a size check INSTEAD of the read.
    let hash_at = body.find("std::io::copy").expect("checked above");
    let compare_at = body.find("if sha != ").expect("the comparison is in the gate");
    assert!(hash_at < compare_at, "the comparison must be against the digest this run computed");
}

/// The registered-class path holds the same rule for a class this build has no row for: the
/// holding is admitted by a COMPUTED digest equal to the registered root, never by a declared one.
#[test]
fn the_registered_class_path_admits_by_a_computed_digest() {
    let source =
        std::fs::read_to_string(repo_root().join("misaka-palw-sdk/src/sdk.rs")).expect("misaka-palw-sdk/src/sdk.rs is in the tree");
    assert!(source.contains("fn resolve_chain_registered"), "the registered-class path still exists");
    let step = source.split("fn resolve_chain_registered").nth(1).expect("checked above");
    let body = &step[..step.len().min(4_000)];
    assert!(
        body.contains("artifact_root"),
        "the registered-class path admits a holding by its registered root, computed from the bytes"
    );
}

/// **SA-7, the RELAY form.** [`nothing_in_the_palw_path_logs_a_prompt`] is name-based — it looks
/// for `rendered_prompt`, `prompt_token_ids` and their siblings — and it therefore cannot see the
/// shape both live findings actually took: **forwarding another process's stderr.** A worker that
/// echoes any part of its input while failing turns its supervisor's helpful relay into a
/// disclosure, and no name in the supervisor's source says "prompt" anywhere on that path.
///
/// So this scan bans the relay itself. Two boot paths keep theirs, because at boot there is no job
/// in the process and therefore no user input to leak; everything else is tracked with what closes
/// it, and a tracked entry that no longer matches FAILS, so the list cannot outlive its subjects.
#[test]
fn no_shipped_path_relays_a_worker_s_stderr_unless_it_is_named_here() {
    /// The relay form, in the spellings this tree uses.
    const RELAY: &[&str] = &["from_utf8_lossy(&stderr", "from_utf8_lossy(&err", "\"[palw-worker] {line}\""];

    /// Permanently exempt: `(file, a substring of the offending line, why it is allowed)`.
    const ALLOWED: &[(&str, &str, &str)] = &[
        (
            "misaka-palw-gateway/src/main.rs",
            "[palw-worker] {line}",
            "the gateway's per-job stderr relay: the pipe is always drained, the lines are printed only \
             under MISAKA_PALW_GATEWAY_LOG_WORKER_STDERR=1 (consent), else one count line (ADR-0079 SA-7)",
        ),
        (
            "misaka-palw-agent/src/agent.rs",
            "worker v2-manifest probe failed",
            "BOOT: `run_captured`'s manifest probe, before any job exists. The supervisor is \
             starting; no prompt has entered the process, and an operator whose worker will not \
             report its manifest needs the worker's own reason.",
        ),
        (
            "misaka-palw-agent/src/agent.rs",
            "from_utf8_lossy(&stderr[tail_start..])",
            "BOOT: the golden selftest's stderr tail on the quarantine path. Same reason — it runs \
             once at startup over the REGISTERED golden vectors, which are chain data, not a \
             stranger's text.",
        ),
    ];

    /// Tolerated for now, each with the thing that closes it. A tracked line that no longer
    /// matches is a stale exemption and fails below: the list cannot rot into a blanket.
    const TRACKED: &[(&str, &str, &str)] = &[
        (
            "misaka-palw/src/lib.rs",
            "stderr: String::from_utf8_lossy(&stderr).trim()",
            "PER JOB: `PalwError::WorkerFailed` carries the worker's stderr into an error whose \
             Display a node logs. Same shape as the entry above and closes the same way.",
        ),
    ];

    let mut findings = Vec::new();
    let mut matched: Vec<(String, String)> = Vec::new();
    for (path, source) in rust_sources() {
        let rel = relative(&path);
        // Test modules construct these deliberately; the rule is about the shipped path.
        let shipped = source.split("#[cfg(test)]").next().unwrap_or(&source).to_string();
        for (n, line) in shipped.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
                continue;
            }
            if !RELAY.iter().any(|r| line.contains(r)) {
                continue;
            }
            let named = ALLOWED.iter().chain(TRACKED.iter()).find(|(f, needle, _)| *f == rel && line.contains(needle));
            match named {
                Some((f, needle, _)) => matched.push(((*f).to_string(), (*needle).to_string())),
                None => findings.push(format!("{rel}:{}: relays another process's stderr — {}", n + 1, line.trim())),
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0079 SA-7: a supervisor that forwards its worker's stderr forwards whatever the \
         worker echoed of its input. A new one is either a BOOT path (add it to ALLOWED with that \
         reason) or a job path (fix it, or add it to TRACKED with what closes it). Found:\n  {}",
        findings.join("\n  ")
    );
    for (file, needle, _) in ALLOWED.iter().chain(TRACKED.iter()) {
        assert!(
            matched.iter().any(|(f, n)| f == file && n == needle),
            "the exemption {file} / {needle:?} matches nothing any more — delete it rather than \
             leave a name standing in for a line that is gone"
        );
    }
}

/// **S12, at tree level.** `declare_backend_in_force` is the one function that makes
/// `security-report` and the boot line say something other than `none`. It has to be `pub`,
/// because the backends live in child modules of `host_security` and one of them is a separate
/// file — and `pub` makes it callable from anywhere in the workspace. So the guard is here: the
/// only call sites are the installers themselves, each of which has just watched its own drill
/// deny what it promises. A call from a supervisor, a CLI, or a test fixture would be a report
/// naming a backend nobody proved, which is precisely the failure S12 exists to forbid.
#[test]
fn only_an_installer_declares_a_backend_in_force() {
    /// The files that ARE the installers: each declares a backend only after its drill returned.
    const INSTALLERS: &[&str] = &["misaka-palw/src/host_security.rs", "misaka-palw/src/host_security_linux.rs"];

    let mut findings = Vec::new();
    let mut declared_in = Vec::new();
    for (path, source) in rust_sources() {
        let rel = relative(&path);
        for (n, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
                continue;
            }
            if !line.contains("declare_backend_in_force(") {
                continue;
            }
            declared_in.push(rel.clone());
            if !INSTALLERS.contains(&rel.as_str()) {
                findings.push(format!("{rel}:{}: declares a backend in force outside the installers", n + 1));
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0079 S12: the backend actually in force is declared by the installer that PROVED it, and by nothing \
         else. Found:\n  {}",
        findings.join("\n  ")
    );
    assert!(
        declared_in.iter().any(|f| f == "misaka-palw/src/host_security.rs"),
        "the declaration point itself must still exist in host_security.rs — this guard is empty if it does not"
    );
}

/// **SA-7 — nothing logs a prompt.** The gateway, the supervisor, the worker and the seat log
/// token counts and roots. A log line that interpolates prompt text or prompt ids turns "private
/// unless disputed" into a disclosure by default.
#[test]
fn nothing_in_the_palw_path_logs_a_prompt() {
    const PROMPT_CARRYING: &[&str] =
        &["rendered_prompt", "prompt_token_ids", "chat.messages", "message.content", "msg.content", "prompt_text", "input.text"];
    const LOGGERS: &[&str] =
        &["eprintln!", "println!", "log::info!", "log::warn!", "log::debug!", "log::trace!", "info!(", "warn!(", "debug!(", "trace!("];

    let mut findings = Vec::new();
    for (path, source) in rust_sources() {
        // Test modules assert on prompts by construction; the rule is about the shipped path.
        let shipped = source.split("#[cfg(test)]").next().unwrap_or(&source).to_string();
        for (n, line) in shipped.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
                continue;
            }
            if !LOGGERS.iter().any(|l| line.contains(l)) {
                continue;
            }
            for carrier in PROMPT_CARRYING {
                if line.contains(carrier) {
                    findings.push(format!("{}:{}: a log line carries `{carrier}`", relative(&path), n + 1));
                }
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0079 SA-7 / ADR-0077 SA-5: the gateway, supervisor, worker and seat log no prompt text and no prompt \
         ids by default. Found:\n  {}",
        findings.join("\n  ")
    );
}
