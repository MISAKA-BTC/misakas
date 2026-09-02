//! **`misaka node security-report`** — ADR-0079 Decision 13 and S12.
//!
//! The posture is a local report, printed by the node, **signed by nobody**. It is not a chain
//! object, it earns nothing, and Decision 2 is why: the chain cannot observe whether a host ran
//! confined, so a confinement claim on the chain is a vote, and votes are what this lineage
//! refuses.
//!
//! What makes this report worth reading is the one rule it holds throughout: **it prints what is
//! in force, never what was configured.** The backend is read from the process that would install
//! it, not from a flag. The worker environment is MEASURED by spawning a child through the real
//! hardening and reading back what the child got. The listening sockets are the kernel's, not the
//! config file's. The interpreter fence is read off the running node's own argv. Where a fact
//! cannot be obtained on this platform the row says `unavailable` and says why — an honest gap
//! beats a confident guess, and S12 is exactly the assertion that a disabled backend reports
//! `none` rather than the configured value.
//!
//! Per SA-7 it prints **no key material and no prompts**: a process that holds a key is named, and
//! the flag that points at its key file is named, and the value never is.
//!
//! Exit codes reuse `node liveness`'s discipline — 0 when nothing is wrong, a distinct code per
//! verdict otherwise:
//!
//! * `SECURITY_EXPOSED` (13) — a public entrance on a host with no confinement backend, or a
//!   process that parses public input while holding key material. Decision 10's two refusals,
//!   observed after the fact.
//! * `SECURITY_DEGRADED` (14) — no backend in force, but nothing public is exposed behind it.

use std::path::{Path, PathBuf};
use std::process::Command;

use misaka_palw::host_security::{
    ALLOW_PUBLIC_GATEWAY_ENV, PALW_WORKER_ENV_ALLOWLIST, PALW_WORKER_ENV_PINNED, confinement_backend_available, establish_confinement,
    harden_worker_command, resident_bytes, worker_max_resident_bytes, worker_working_dir,
};

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Ok,
    Info,
    Warn,
    Fail,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Ok => "OK",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Fail => "FAIL",
        }
    }
}

struct Row {
    section: &'static str,
    label: String,
    value: String,
    level: Level,
}

fn row(section: &'static str, label: impl Into<String>, value: impl Into<String>, level: Level) -> Row {
    Row { section, label: label.into(), value: value.into(), level }
}

// ---------------------------------------------------------------------------------------------
// Live process table
// ---------------------------------------------------------------------------------------------

/// One live process, as the OS reports it. `args` is the command line WITHOUT any value that
/// could be a secret — see [`redact_args`].
struct LiveProcess {
    pid: u32,
    name: String,
    args: Vec<String>,
}

/// Every process this report knows how to say anything about.
const PALW_PROCESS_NAMES: &[&str] =
    &["kaspad", "palw-agent", "misaka-palw-gateway", "misaka-palw-fp-rail", "palw-worker", "kaspa-pq-signer", "kaspa-pq-validator"];

/// **SA-7.** A flag whose VALUE would name or carry key material keeps its name and loses its
/// value. The report says "this process was pointed at a key", never where or which.
const SECRET_BEARING_FLAGS: &[&str] =
    &["--bond-key-seed", "--seed", "--key", "--private-key", "--keyfile", "--validator-seed", "--mnemonic", "--password"];

fn redact_args(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut redact_next = false;
    for token in raw.split_whitespace() {
        if redact_next {
            out.push("<redacted>".to_string());
            redact_next = false;
            continue;
        }
        if let Some((flag, _)) = token.split_once('=')
            && SECRET_BEARING_FLAGS.contains(&flag)
        {
            out.push(format!("{flag}=<redacted>"));
            continue;
        }
        if SECRET_BEARING_FLAGS.contains(&token) {
            redact_next = true;
        }
        out.push(token.to_string());
    }
    out
}

/// The live process table, filtered to the PALW path. `None` when the host has no `ps` this
/// report can read — reported as `unavailable`, never guessed at.
fn live_processes() -> Option<Vec<LiveProcess>> {
    #[cfg(unix)]
    {
        // Absolute path: this report must not depend on the operator's `PATH` to describe the
        // machine, which is the same reason the worker does not get one.
        let out = Command::new("/bin/ps").args(["-Ao", "pid=,comm=,args="]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut found = Vec::new();
        for line in text.lines() {
            let mut parts = line.trim_start().splitn(3, char::is_whitespace);
            let pid: u32 = parts.next()?.parse().ok()?;
            let comm = parts.next().unwrap_or_default();
            let args = parts.next().unwrap_or_default();
            let base = Path::new(comm).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| comm.to_string());
            if PALW_PROCESS_NAMES.iter().any(|n| base == *n) {
                found.push(LiveProcess { pid, name: base, args: redact_args(args) });
            }
        }
        Some(found)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Live listening sockets
// ---------------------------------------------------------------------------------------------

struct Listener {
    bind: String,
    process: String,
    pid: Option<u32>,
}

impl Listener {
    fn is_public(&self) -> bool {
        !misaka_palw::host_security::listen_is_loopback(&self.bind)
    }
}

/// The kernel's answer, not the config file's. macOS gets `lsof`; Linux gets `ss` when it is
/// installed and `/proc/net/tcp{,6}` otherwise (which knows the bind but not the process).
fn live_listeners() -> Option<Vec<Listener>> {
    #[cfg(target_os = "macos")]
    {
        for lsof in ["/usr/sbin/lsof", "/usr/bin/lsof"] {
            let Ok(out) = Command::new(lsof).args(["-nP", "-iTCP", "-sTCP:LISTEN"]).output() else { continue };
            if !out.status.success() {
                continue;
            }
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let mut found = Vec::new();
            for line in text.lines().skip(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 9 {
                    continue;
                }
                let bind = fields[8].trim_end_matches(" (LISTEN)").to_string();
                found.push(Listener { bind, process: fields[0].to_string(), pid: fields[1].parse().ok() });
            }
            return Some(found);
        }
        None
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = Command::new("/usr/bin/ss").args(["-ltnH", "-p"]).output()
            && out.status.success()
        {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let mut found = Vec::new();
            for line in text.lines() {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 4 {
                    continue;
                }
                let bind = fields[3].to_string();
                let process = fields
                    .get(5)
                    .and_then(|p| p.split("((\"").nth(1))
                    .and_then(|p| p.split('"').next())
                    .unwrap_or("unknown")
                    .to_string();
                let pid =
                    fields.get(5).and_then(|p| p.split("pid=").nth(1)).and_then(|p| p.split(',').next()).and_then(|p| p.parse().ok());
                found.push(Listener { bind, process, pid });
            }
            return Some(found);
        }
        let mut found = Vec::new();
        for (path, v6) in [("/proc/net/tcp", false), ("/proc/net/tcp6", true)] {
            let Ok(text) = std::fs::read_to_string(path) else { continue };
            for line in text.lines().skip(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 4 || fields[3] != "0A" {
                    continue;
                }
                let Some((addr_hex, port_hex)) = fields[1].split_once(':') else { continue };
                let Ok(port) = u16::from_str_radix(port_hex, 16) else { continue };
                let addr = if v6 { format!("[{addr_hex}]") } else { decode_proc_ipv4(addr_hex) };
                found.push(Listener {
                    bind: format!("{addr}:{port}"),
                    process: "unknown (install iproute2 for names)".into(),
                    pid: None,
                });
            }
        }
        if found.is_empty() { None } else { Some(found) }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn decode_proc_ipv4(hex: &str) -> String {
    let Ok(raw) = u32::from_str_radix(hex, 16) else { return hex.to_string() };
    let b = raw.to_le_bytes();
    format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
}

// ---------------------------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------------------------

/// **`misaka node security-report`.** See the module doc for what it promises and what it refuses.
pub fn security_report(ctx: &Ctx, worker: Option<&PathBuf>, verify_artifacts: bool) -> CliResult {
    let mut rows: Vec<Row> = Vec::new();
    let mut json = serde_json::Map::new();

    // --- 1. the confinement backend, IN FORCE ------------------------------------------------
    //
    // Measured, not read: this runs the backend's OWN drill on this host and reports what came
    // back. A configured backend that cannot install its denials comes back `none` with the
    // reason, which is exactly S12. A report that printed `MISAKA_PALW_CONFINEMENT` would be
    // printing what someone typed.
    let probe_workdir = worker_working_dir(None);
    let (confinement, drill_notes) = match probe_workdir.as_ref() {
        Ok(dir) => establish_confinement(dir, std::slice::from_ref(dir)),
        Err(e) => (misaka_palw::host_security::Confinement::none(), vec![format!("no working directory to drill in: {e}")]),
    };
    let in_force = confinement.backend();
    let available = confinement_backend_available();
    for note in &drill_notes {
        rows.push(row("confinement", "drill", note.clone(), if note.contains("FAILED") { Level::Warn } else { Level::Info }));
    }
    rows.push(row(
        "confinement",
        "backend in force",
        in_force.name(),
        if in_force == misaka_palw::host_security::ConfinementBackend::None { Level::Warn } else { Level::Ok },
    ));
    rows.push(row("confinement", "backend this build could install", available.name(), Level::Info));
    if in_force == misaka_palw::host_security::ConfinementBackend::None {
        rows.push(row(
            "confinement",
            "consequence",
            "the environment discipline still applies; a PUBLIC gateway bind refuses to start (ADR-0079 Decision 10)",
            Level::Info,
        ));
    }
    json.insert(
        "confinement".into(),
        serde_json::json!({
            "in_force": in_force.name(),
            "available": available.name(),
            "requested": std::env::var(misaka_palw::host_security::PALW_CONFINEMENT_ENV).unwrap_or_else(|_| "none".into()),
            "drill": drill_notes,
            "reported_from": "this host's own drill, run now",
        }),
    );

    // --- 2. the worker environment, AS THE CHILD RECEIVED IT ---------------------------------
    let workdir = probe_workdir;
    let (delivered, measured_by) = measure_delivered_environment(workdir.as_deref().ok());
    for line in &delivered {
        rows.push(row("environment", "delivered", line.clone(), Level::Ok));
    }
    rows.push(row("environment", "measured by", measured_by, Level::Info));
    rows.push(row(
        "environment",
        "allowlist",
        format!(
            "{} names + {} pinned (PATH is NOT among them — ADR-0079 SA-4)",
            PALW_WORKER_ENV_ALLOWLIST.len(),
            PALW_WORKER_ENV_PINNED.len()
        ),
        Level::Info,
    ));
    match &workdir {
        Ok(dir) => rows.push(row("environment", "worker working directory", dir.display().to_string(), Level::Ok)),
        Err(e) => rows.push(row("environment", "worker working directory", e.clone(), Level::Fail)),
    }
    let (_, measure) = resident_bytes(std::process::id());
    rows.push(row(
        "environment",
        "resident ceiling",
        format!("{} bytes, measured as {}", worker_max_resident_bytes(), measure.name()),
        Level::Info,
    ));
    json.insert(
        "worker_environment".into(),
        serde_json::json!({
            "delivered": delivered,
            "measured_by": measured_by,
            "allowlist": PALW_WORKER_ENV_ALLOWLIST,
            "pinned": PALW_WORKER_ENV_PINNED.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>(),
            "path_is_excluded": true,
            "working_directory": workdir.as_ref().map(|d| d.display().to_string()).unwrap_or_else(|e| e.clone()),
            "max_resident_bytes": worker_max_resident_bytes(),
            "resident_measure": measure.name(),
        }),
    );

    // --- 3. live processes and who holds key material -----------------------------------------
    let processes = live_processes();
    let mut key_holders = Vec::new();
    match &processes {
        None => rows.push(row("processes", "process table", "unavailable on this host", Level::Info)),
        Some(list) if list.is_empty() => rows.push(row("processes", "PALW processes", "none running", Level::Info)),
        Some(list) => {
            for p in list {
                let holds = process_holds_key_material(p);
                if holds.is_some() {
                    key_holders.push(p.name.clone());
                }
                rows.push(row(
                    "processes",
                    format!("{} (pid {})", p.name, p.pid),
                    format!("confinement {} | key material: {}", in_force.name(), holds.clone().unwrap_or_else(|| "none".into())),
                    if holds.is_some() && parses_public_input(&p.name) { Level::Fail } else { Level::Ok },
                ));
            }
        }
    }
    json.insert(
        "processes".into(),
        serde_json::json!(
            processes
                .as_ref()
                .map(|l| l
                    .iter()
                    .map(|p| serde_json::json!({
                        "pid": p.pid,
                        "name": p.name,
                        "confinement_backend": in_force.name(),
                        "holds_key_material": process_holds_key_material(p),
                        "parses_public_input": parses_public_input(&p.name),
                        "argv": p.args,
                    }))
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        ),
    );

    // --- 4. listening sockets, from the kernel ------------------------------------------------
    // `lsof` reports one row per protocol family; the same bind twice tells an operator nothing
    // twice, and the JSON and the human table must not disagree about how many sockets are open.
    let listeners = live_listeners().map(|mut list| {
        let mut seen = std::collections::BTreeSet::new();
        list.retain(|l| seen.insert((l.bind.clone(), l.pid)));
        list
    });
    let ack_given = std::env::var(ALLOW_PUBLIC_GATEWAY_ENV).map(|v| v == "1").unwrap_or(false);
    let mut public_gateway = false;
    match &listeners {
        None => rows.push(row("listeners", "socket table", "unavailable on this host (install lsof or iproute2)", Level::Info)),
        Some(list) => {
            for l in list {
                let gateway = l.process.contains("gateway");
                let public = l.is_public();
                if gateway && public {
                    public_gateway = true;
                }
                rows.push(row(
                    "listeners",
                    format!("{} ({}{})", l.bind, l.process, l.pid.map(|p| format!(" pid {p}")).unwrap_or_default()),
                    match (public, gateway) {
                        // The one row where the acknowledgement variable is the rule: a PALW
                        // gateway on a public bind. Everything else on this host is somebody
                        // else's listener, reported because an operator asked what is open.
                        (true, true) => format!("PUBLIC PALW ENTRANCE — {ALLOW_PUBLIC_GATEWAY_ENV} required, given: {ack_given}"),
                        (true, false) => "public (not a PALW entrance)".to_string(),
                        (false, _) => "loopback".to_string(),
                    },
                    match (public, gateway) {
                        (true, true) => Level::Warn,
                        (true, false) => Level::Info,
                        (false, _) => Level::Ok,
                    },
                ));
            }
        }
    }
    json.insert(
        "listeners".into(),
        serde_json::json!(
            listeners
                .as_ref()
                .map(|l| l
                    .iter()
                    .map(|s| serde_json::json!({
                        "bind": s.bind,
                        "process": s.process,
                        "pid": s.pid,
                        "public": s.is_public(),
                        "acknowledgement_variable": ALLOW_PUBLIC_GATEWAY_ENV,
                        "acknowledgement_required": s.is_public() && s.process.contains("gateway"),
                        "acknowledgement_given": ack_given,
                    }))
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        ),
    );
    json.insert("key_material_holders".into(), serde_json::json!(key_holders));

    // --- 5. artifacts, verified at load with their COMPUTED digests --------------------------
    let artifacts = artifact_section(worker, workdir.as_deref().ok(), verify_artifacts, &mut rows);
    json.insert("artifacts".into(), artifacts);

    // --- 6. the interpreter fence, read off the running node's own argv -----------------------
    let fence = interpreter_fence_state(processes.as_deref());
    rows.push(row("interpreter fence", "chain-registered class arm (ADR-0067 Decision 5)", fence.clone(), Level::Info));
    json.insert("interpreter_fence".into(), serde_json::json!({ "state": fence, "read_from": "the running node's argv" }));

    // --- verdict ------------------------------------------------------------------------------
    let mut findings: Vec<String> = Vec::new();
    let mut code = exit::SUCCESS;
    if public_gateway && in_force == misaka_palw::host_security::ConfinementBackend::None {
        findings.push("a PUBLIC gateway is listening on a host whose confinement backend is `none` (ADR-0079 Decision 10)".into());
        code = exit::SECURITY_EXPOSED;
    }
    for holder in &key_holders {
        if parses_public_input(holder) {
            findings.push(format!("{holder} parses public input AND holds key material (ADR-0079 Decision 4)"));
            code = exit::SECURITY_EXPOSED;
        }
    }
    if code == exit::SUCCESS && in_force == misaka_palw::host_security::ConfinementBackend::None {
        findings.push("no platform confinement backend is in force; the environment discipline is the whole of it".into());
        code = exit::SECURITY_DEGRADED;
    }
    let verdict = match code {
        c if c == exit::SECURITY_EXPOSED => "EXPOSED",
        c if c == exit::SECURITY_DEGRADED => "DEGRADED",
        _ => "OK",
    };
    json.insert("findings".into(), serde_json::json!(findings));
    json.insert("verdict".into(), serde_json::json!(verdict));
    json.insert("exit".into(), serde_json::json!(code));

    match ctx.output {
        OutputFormat::Json => println!("{}", serde_json::Value::Object(json)),
        OutputFormat::Human => {
            println!("misaka node security-report — printed from live state, signed by nobody (ADR-0079 Decision 13)");
            let mut current = "";
            for r in &rows {
                if r.section != current {
                    println!("\n[{}]", r.section);
                    current = r.section;
                }
                println!("  {:<5} {:<44} {}", r.level.tag(), r.label, r.value);
            }
            println!("\n{verdict}");
            for f in &findings {
                println!("  - {f}");
            }
        }
    }

    if code == exit::SUCCESS { Ok(()) } else { Err(CliError::new(code, format!("security-report: {verdict}"))) }
}

/// Spawn a child through the REAL hardening and read back what it got. A constant this process
/// prints is what someone configured; a child's own environment is what is in force.
fn measure_delivered_environment(workdir: Option<&Path>) -> (Vec<String>, &'static str) {
    let computed = misaka_palw::host_security::worker_environment();
    let Some(workdir) = workdir else {
        return (computed.as_env_lines(), "computed (no working directory could be created)");
    };
    let env_bin = ["/usr/bin/env", "/bin/env"].iter().map(PathBuf::from).find(|p| p.is_file());
    let Some(env_bin) = env_bin else {
        return (computed.as_env_lines(), "computed (no /usr/bin/env on this host)");
    };
    let mut cmd = Command::new(&env_bin);
    harden_worker_command(&mut cmd, workdir);
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let mut lines: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
            lines.sort();
            (lines, "measured: a child spawned through the real hardening printed it")
        }
        _ => (computed.as_env_lines(), "computed (the probe child did not run)"),
    }
}

/// Which live processes hold key material, from their own argv and their role. The FLAG is named;
/// its value never is (SA-7).
fn process_holds_key_material(p: &LiveProcess) -> Option<String> {
    if p.name == "kaspa-pq-signer" {
        return Some("the ML-DSA-87 secret (this is the signer sidecar — the one process that should)".into());
    }
    let flags: Vec<&String> =
        p.args.iter().filter(|a| SECRET_BEARING_FLAGS.iter().any(|f| a.as_str() == *f || a.starts_with(&format!("{f}=")))).collect();
    if flags.is_empty() {
        return None;
    }
    Some(format!("pointed at a key by {} (value not shown)", flags.iter().map(|f| f.as_str()).collect::<Vec<_>>().join(", ")))
}

/// Does this process parse a stranger's bytes? Decision 4's other column.
fn parses_public_input(name: &str) -> bool {
    name == "misaka-palw-gateway"
}

fn interpreter_fence_state(processes: Option<&[LiveProcess]>) -> String {
    let Some(list) = processes else {
        return "unknown (no process table on this host)".into();
    };
    let Some(node) = list.iter().find(|p| p.name == "kaspad") else {
        return "no kaspad running — the fence is a property of a live node".into();
    };
    if node.args.iter().any(|a| a == "--palw-chain-classes") {
        "ARMED (--palw-chain-classes): this node interprets class declarations written by strangers".into()
    } else {
        "SEALED (ADR-0067 Decision 5 default): chain-registered class declarations are refused".into()
    }
}

/// Artifact roots verified at load, with their computed digests. The manifest probe is the
/// worker's own answer about what it hashed; `--verify-artifacts` re-reads the bytes here, which
/// costs a full pass over a possibly 33 GiB file and is therefore opt-in and said so.
fn artifact_section(worker: Option<&PathBuf>, workdir: Option<&Path>, verify: bool, rows: &mut Vec<Row>) -> serde_json::Value {
    let mut paths = serde_json::Map::new();
    for name in PALW_WORKER_ENV_ALLOWLIST {
        if let Ok(value) = std::env::var(name) {
            let present = Path::new(&value).is_file();
            rows.push(row(
                "artifacts",
                *name,
                format!("{value} ({})", if present { "present" } else { "MISSING" }),
                if present { Level::Ok } else { Level::Warn },
            ));
            let digest = if verify && present { compute_sha256(Path::new(&value)) } else { None };
            if let Some(d) = &digest {
                rows.push(row("artifacts", format!("{name} computed sha256"), d.clone(), Level::Ok));
            }
            paths.insert((*name).to_string(), serde_json::json!({ "path": value, "present": present, "computed_sha256": digest }));
        }
    }
    if paths.is_empty() {
        rows.push(row("artifacts", "artifact paths", "no MISAKA_PALW_* artifact variable is set in this shell", Level::Info));
    }
    if !verify {
        rows.push(row("artifacts", "digests", "not recomputed this run (pass --verify-artifacts; it is a full read)", Level::Info));
    }

    let manifest = match (worker, workdir) {
        (Some(worker), Some(workdir)) => {
            let mut cmd = Command::new(worker);
            cmd.args(["--mode", "v3-manifest"]);
            harden_worker_command(&mut cmd, workdir);
            match cmd.output() {
                Ok(out) if out.status.success() => serde_json::from_slice::<serde_json::Value>(&out.stdout).ok(),
                _ => {
                    let mut cmd = Command::new(worker);
                    cmd.args(["--mode", "v2-manifest"]);
                    harden_worker_command(&mut cmd, workdir);
                    match cmd.output() {
                        Ok(out) if out.status.success() => serde_json::from_slice::<serde_json::Value>(&out.stdout).ok(),
                        _ => None,
                    }
                }
            }
        }
        _ => None,
    };
    match &manifest {
        Some(doc) => {
            for key in ["runtime_manifest_hash", "runtime_manifest_hash_v2", "model_profile_id", "golden_vector_root"] {
                if let Some(v) = doc.get(key).and_then(|v| v.as_str()) {
                    rows.push(row("artifacts", key, v.to_string(), Level::Ok));
                }
            }
        }
        None if worker.is_some() => {
            rows.push(row("artifacts", "worker manifest", "the worker did not answer a manifest probe", Level::Warn))
        }
        None => rows.push(row("artifacts", "worker manifest", "not probed (pass --worker <palw-worker>)", Level::Info)),
    }

    serde_json::json!({ "paths": paths, "verified_this_run": verify, "worker_manifest": manifest })
}

/// The SAME digest the worker's own gate computes (`pinned_model_path_v2`), so an operator can
/// compare this report's number against the pin without a second algorithm in the way. It is a
/// full read of the file, which is why it is opt-in: the hybrid class's artifact is 33 GiB.
fn compute_sha256(path: &Path) -> Option<String> {
    use sha2::Digest;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).ok()?;
    Some(faster_hex::hex_string(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **SA-7.** A flag that points at a key keeps its name and loses its value, in both spellings.
    #[test]
    fn secret_bearing_flag_values_are_redacted() {
        let args = redact_args("kaspad --palw-chain-classes --bond-key-seed /srv/secret/bond.seed --rpclisten 127.0.0.1:16210");
        assert!(args.contains(&"--bond-key-seed".to_string()), "the flag is named");
        assert!(args.contains(&"<redacted>".to_string()), "its value is not");
        assert!(!args.iter().any(|a| a.contains("bond.seed")), "no path to key material is printed");

        let joined = redact_args("misaka-palw-fp-rail --bond-key-seed=/srv/secret/bond.seed");
        assert!(joined.contains(&"--bond-key-seed=<redacted>".to_string()));
        assert!(!joined.iter().any(|a| a.contains("/srv/secret")));
    }

    /// **S12.** With no installer having declared a backend, the report says `none` — not the
    /// value someone configured, and not what the platform could support.
    #[test]
    fn the_report_says_none_when_no_backend_is_in_force() {
        // SAFETY: single-threaded test setup.
        unsafe { std::env::remove_var(misaka_palw::host_security::PALW_CONFINEMENT_ENV) };
        let dir = std::env::temp_dir();
        let (confinement, notes) = establish_confinement(&dir, &[]);
        assert_eq!(confinement.backend().name(), "none");
        assert!(notes.iter().any(|n| n.contains("no backend requested")), "{notes:?}");
        // What the host COULD install is a separate row that never stands in for what is in
        // force: it is reported, and it is one of the three names, and it changes nothing above.
        assert!(
            ["none", "macos-sandbox-exec", "linux-seccomp-landlock"].contains(&confinement_backend_available().name()),
            "the available backend must be a named value"
        );
    }

    /// Decision 4's table, as this report reads it: the gateway is the process that parses public
    /// input, and the signer is the process that holds the key. They are never the same one.
    #[test]
    fn the_public_parser_and_the_key_holder_are_different_processes() {
        assert!(parses_public_input("misaka-palw-gateway"));
        for keyless in ["palw-agent", "palw-worker", "kaspa-pq-signer", "kaspad"] {
            assert!(!parses_public_input(keyless), "{keyless} does not parse public HTTP text");
        }
        let signer = LiveProcess { pid: 1, name: "kaspa-pq-signer".into(), args: vec![] };
        assert!(process_holds_key_material(&signer).is_some());
        let gateway =
            LiveProcess { pid: 2, name: "misaka-palw-gateway".into(), args: vec!["--listen".into(), "127.0.0.1:8790".into()] };
        assert!(process_holds_key_material(&gateway).is_none(), "the gateway holds the executor PUBLIC key only");
    }

    /// The fence is read off the running node's own argv, and says so when there is no node.
    #[test]
    fn the_interpreter_fence_is_read_from_live_argv() {
        assert!(interpreter_fence_state(None).starts_with("unknown"));
        assert!(interpreter_fence_state(Some(&[])).contains("no kaspad running"));
        let sealed = [LiveProcess { pid: 3, name: "kaspad".into(), args: vec!["--utxoindex".into()] }];
        assert!(interpreter_fence_state(Some(&sealed)).starts_with("SEALED"));
        let armed = [LiveProcess { pid: 3, name: "kaspad".into(), args: vec!["--palw-chain-classes".into()] }];
        assert!(interpreter_fence_state(Some(&armed)).starts_with("ARMED"));
    }
}
