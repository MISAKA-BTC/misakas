//! Host-side confinement for every process that executes a PALW class (ADR-0079).
//!
//! **This module is consensus-inert by construction** (ADR-0079 Decision 2). Nothing here enters
//! a priced, committed or certified struct; nothing here can change a root. Two hosts with
//! different backends compute the same numbers or the job fails — a security control that can
//! change an arithmetic result is a fork risk, and this lineage has already lost a fleet to a
//! gate that measured the wrong side.
//!
//! What it owns:
//!
//! * **[`PALW_WORKER_ENV_ALLOWLIST`]** — the exact environment a worker child receives. The spawn
//!   is `env_clear()`'d and the child gets the allowlist ∩ parent, plus the pinned locale values.
//!   Everything else in the operator's environment (`SSH_AUTH_SOCK`, a cloud token, a wallet
//!   path, an exchange API key) stops at the supervisor. `PATH` is deliberately NOT on the list:
//!   the supervisor spawns by absolute path, and an inherited `PATH` is an execution vector on
//!   every platform without an `execve` denial (ADR-0079 security amendment SA-4).
//! * **[`worker_working_dir`]** — an explicit working directory that is neither the operator's
//!   home nor the node's datadir.
//! * **[`PALW_WORKER_MAX_RESIDENT_BYTES`]** — a per-job memory ceiling, named for what it
//!   measures. NOT `RLIMIT_AS`: the hybrid class maps a 33 GiB artifact, so an address-space cap
//!   at any resident-shaped value kills the worker at startup (SA-1). Mapped file pages are not
//!   the process's to be charged for twice, so the Linux measure is `RssAnon`.
//! * **[`ConfinementBackend`]** — what is ACTUALLY in force, never what was configured. A host
//!   with no backend reports `none` and may still mine; ADR-0079 Decision 10 is what refuses to
//!   let it be a public entrance.
//!
//! The allowlist is a constant in the tree, not a config file, so adding to it is a reviewed act.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The Linux backend: `seccomp` + `Landlock`, in its own file because it is long and exists on one
/// platform. It is a child module, so it reaches this module's private constructors — which is the
/// point: [`Confinement`] must stay unconstructible from outside the installers.
#[cfg(target_os = "linux")]
#[path = "host_security_linux.rs"]
mod linux;

// -------------------------------------------------------------------------------------------
// Decision 5 / S2 — the environment a worker child receives
// -------------------------------------------------------------------------------------------

/// The ONLY variable names forwarded from the supervisor's environment to a worker child.
///
/// Each name is here because a shipped worker binary reads it at runtime:
///
/// | name | read by |
/// |---|---|
/// | `MISAKA_PALW_GGUF` | `misaka-palw-worker` (`pinned_model_path_v2`), the Qwen3.6 FP worker's tokenizer source |
/// | `MISAKA_PALW_GOLDEN` | `misaka-palw-worker`'s boot golden gate |
/// | `MISAKA_PALW_ARTIFACT` | the A16 / Qwen3.6 free-prompt workers' pinned artifact |
/// | `MISAKA_PALW_TOKENIZER` | the A16 free-prompt worker's tokenizer |
/// | `MISAKA_PALW_MODEL_ID` | the Qwen3.6 free-prompt worker's registered model id |
/// | `MISAKA_PALW_NETWORK_ID` | every free-prompt worker, as an INPUT to the context hash |
///
/// **`MISAKA_PALW_NETWORK_ID` was missing, and its absence made the free-prompt workers
/// unstartable through a gateway** — measured 2026-09-03, where the A16 worker refused with
/// "MISAKA_PALW_NETWORK_ID is not set" on every spawn and the gateway reported only that the
/// worker "exited before announcing its manifest". Two well-founded rules had been left pointing
/// opposite ways: the worker demands the name because every committed root hangs off a context
/// hash that absorbs it, so a guess yields a claim no seat can replay; this list refuses anything
/// "the arithmetic does not need". The network name IS arithmetic here — it is an input to the
/// hash the court re-derives — so it belongs on the list, and its absence was an omission rather
/// than a policy. It is a name, not a capability: it grants no filesystem, no socket, no library
/// injection, and a wrong value fails closed at the court rather than silently.
///
/// **`PATH` is not on this list and must not be added** (SA-4). Neither is `HOME`, `TMPDIR`,
/// `SSH_AUTH_SOCK`, nor anything else: a capability the arithmetic does not need is not granted
/// "for now" — its absence is a property the court already relies on.
pub const PALW_WORKER_ENV_ALLOWLIST: &[&str] =
    &["MISAKA_PALW_GGUF", "MISAKA_PALW_GOLDEN", "MISAKA_PALW_ARTIFACT", "MISAKA_PALW_TOKENIZER", "MISAKA_PALW_MODEL_ID", "MISAKA_PALW_NETWORK_ID"];

/// Values PINNED by this constant rather than inherited — the locale pins the determinism rules
/// already require. Inheriting them would make the child's number formatting a function of the
/// operator's shell, which is exactly the class of input two honest hosts disagree about.
pub const PALW_WORKER_ENV_PINNED: &[(&str, &str)] = &[("LC_ALL", "C"), ("LANG", "C"), ("LC_NUMERIC", "C"), ("TZ", "UTC")];

/// The delivered environment, and what was dropped to produce it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkerEnvironment {
    /// Exactly what the child receives. Sorted, so a report and a test read the same order.
    pub vars: BTreeMap<String, String>,
    /// Allowlisted names the parent did not have. Not an error — a class that needs none of the
    /// artifact variables is a class whose worker reads none of them.
    pub absent: Vec<String>,
}

impl WorkerEnvironment {
    /// The `KEY=VALUE` lines a child would print from `/usr/bin/env`, sorted. The S2 test
    /// compares the DELIVERED set against this, by equality — not by containment.
    pub fn as_env_lines(&self) -> Vec<String> {
        self.vars.iter().map(|(k, v)| format!("{k}={v}")).collect()
    }
}

/// Build the delivered environment from an arbitrary lookup. Injectable so the S2 test can pin
/// the rule against a synthetic parent environment carrying an SSH agent socket and a cloud token.
pub fn worker_environment_from<F>(lookup: F) -> WorkerEnvironment
where
    F: Fn(&str) -> Option<String>,
{
    let mut vars = BTreeMap::new();
    let mut absent = Vec::new();
    for name in PALW_WORKER_ENV_ALLOWLIST {
        match lookup(name) {
            Some(value) => {
                vars.insert((*name).to_string(), value);
            }
            None => absent.push((*name).to_string()),
        }
    }
    for (name, value) in PALW_WORKER_ENV_PINNED {
        // Pinned wins over inherited by construction: the allowlist does not carry these names.
        vars.insert((*name).to_string(), (*value).to_string());
    }
    WorkerEnvironment { vars, absent }
}

/// The delivered environment for this process's actual environment.
pub fn worker_environment() -> WorkerEnvironment {
    worker_environment_from(|name| std::env::var(name).ok())
}

/// Operator override for the worker's working directory. Validated, not trusted: a value that
/// resolves to the operator's home is refused, because that is the exact thing the pin exists to
/// prevent.
pub const PALW_WORKER_WORKDIR_ENV: &str = "MISAKA_PALW_WORKER_WORKDIR";

/// An explicit working directory that is neither the operator's home nor the node's datadir.
///
/// A worker inherits the supervisor's cwd otherwise, and the supervisor is usually started from a
/// login shell or a systemd unit whose `WorkingDirectory=` is the datadir. A relative path in a
/// hostile profile, a core dump, or a library that writes beside itself then lands in the one
/// directory that holds keys and chain state.
///
/// `datadir`, when known, is refused as well. The directory is created `0700`.
pub fn worker_working_dir(datadir: Option<&Path>) -> Result<PathBuf, String> {
    let chosen = match std::env::var(PALW_WORKER_WORKDIR_ENV) {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => std::env::temp_dir().join(format!("misaka-palw-worker-{}", process_owner_tag())),
    };
    if let Some(home) = home_dir()
        && same_path(&chosen, &home)
    {
        return Err(format!(
            "the worker working directory may not be the operator's home ({}); set {PALW_WORKER_WORKDIR_ENV} to a scratch directory",
            home.display()
        ));
    }
    if let Some(datadir) = datadir
        && same_path(&chosen, datadir)
    {
        return Err(format!(
            "the worker working directory may not be the node's datadir ({}); set {PALW_WORKER_WORKDIR_ENV} to a scratch directory",
            datadir.display()
        ));
    }
    std::fs::create_dir_all(&chosen).map_err(|e| format!("cannot create the worker working directory {}: {e}", chosen.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&chosen, std::fs::Permissions::from_mode(0o700));
    }
    Ok(chosen)
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

fn process_owner_tag() -> String {
    #[cfg(unix)]
    {
        // Not a security value — only a per-user scratch name so two accounts on one host do not
        // collide on a `0700` directory the other cannot enter.
        unsafe { libc::geteuid() }.to_string()
    }
    #[cfg(not(unix))]
    {
        "local".to_string()
    }
}

/// **The one spawn discipline** (ADR-0079 Decision 5, S2). Applies `env_clear()`, the delivered
/// allowlist, and the pinned working directory to a command that is about to become a worker.
///
/// Returns the delivered environment so the caller can print it at boot — the operator sees the
/// set the child actually got, not the set someone meant to configure.
pub fn harden_worker_command(cmd: &mut Command, workdir: &Path) -> WorkerEnvironment {
    let delivered = worker_environment();
    cmd.env_clear();
    for (name, value) in &delivered.vars {
        cmd.env(name, value);
    }
    cmd.current_dir(workdir);
    delivered
}

// -------------------------------------------------------------------------------------------
// Decision 6 / S9 (as corrected by SA-1) — the per-job memory ceiling
// -------------------------------------------------------------------------------------------

/// The per-job resident ceiling, named for what it measures.
///
/// **Not `RLIMIT_AS`.** The hybrid class maps a 33 GiB artifact; an address-space cap at any
/// resident-shaped value kills the worker while it is still mapping. What is capped is the
/// memory the process is actually charged for: `RssAnon` on Linux (mapped file pages are the
/// page cache's, reclaimable, and not the job's to be charged for twice), the resident size on
/// macOS.
///
/// The value is a safety net against the availability attack, not a tuning knob: an OOM killer
/// that reaps the node because a model process spiked costs the attacker one prompt, and this
/// fleet has already measured an 8.4 GB burst inside one minute. Operators with a smaller box
/// set it down; a class with a larger footprint sets it up.
pub const PALW_WORKER_MAX_RESIDENT_BYTES: u64 = 64 * 1024 * 1024 * 1024;

/// Operator override for [`PALW_WORKER_MAX_RESIDENT_BYTES`], in bytes.
pub const PALW_WORKER_MAX_RESIDENT_ENV: &str = "MISAKA_PALW_WORKER_MAX_RESIDENT_BYTES";

/// The effective ceiling: the override when it parses, the constant otherwise.
pub fn worker_max_resident_bytes() -> u64 {
    match std::env::var(PALW_WORKER_MAX_RESIDENT_ENV) {
        Ok(raw) => raw.trim().parse::<u64>().ok().filter(|v| *v > 0).unwrap_or(PALW_WORKER_MAX_RESIDENT_BYTES),
        Err(_) => PALW_WORKER_MAX_RESIDENT_BYTES,
    }
}

/// How a resident measurement was obtained — reported, never inferred.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ResidentMeasure {
    /// `/proc/<pid>/status: RssAnon` — anonymous pages only, so a mapped artifact is not charged.
    LinuxRssAnon,
    /// `/proc/<pid>/statm` resident pages — the fallback when `RssAnon` is absent. Includes
    /// mapped file pages, so a class that maps a large artifact needs a ceiling above it.
    LinuxStatm,
    /// `ps -o rss=` — includes resident file-backed pages, same caveat.
    MacosPsRss,
    /// No measurement is available on this platform; the ceiling cannot bind and says so.
    Unavailable,
}

impl ResidentMeasure {
    pub fn name(self) -> &'static str {
        match self {
            ResidentMeasure::LinuxRssAnon => "linux-rss-anon",
            ResidentMeasure::LinuxStatm => "linux-statm-resident",
            ResidentMeasure::MacosPsRss => "macos-ps-rss",
            ResidentMeasure::Unavailable => "unavailable",
        }
    }
}

/// The resident bytes charged to `pid`, and how they were measured.
pub fn resident_bytes(pid: u32) -> (Option<u64>, ResidentMeasure) {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("RssAnon:") {
                    let kb: u64 = rest.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0);
                    return (Some(kb * 1024), ResidentMeasure::LinuxRssAnon);
                }
            }
        }
        if let Ok(statm) = std::fs::read_to_string(format!("/proc/{pid}/statm")) {
            let pages: u64 = statm.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
            return (Some(pages * page), ResidentMeasure::LinuxStatm);
        }
        (None, ResidentMeasure::LinuxRssAnon)
    }
    #[cfg(target_os = "macos")]
    {
        // Absolute path: this process spawns by absolute path everywhere, and the supervisor's
        // own `PATH` is not something a probe should start depending on.
        let out = Command::new("/bin/ps").args(["-o", "rss=", "-p", &pid.to_string()]).output();
        match out {
            Ok(out) if out.status.success() => {
                let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0);
                (Some(kb * 1024), ResidentMeasure::MacosPsRss)
            }
            _ => (None, ResidentMeasure::MacosPsRss),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        (None, ResidentMeasure::Unavailable)
    }
}

/// Where the ceiling is enforced. `CgroupV2` is the kernel doing it; the watchdog is the
/// supervisor doing it; `None` is honest about a platform that can do neither.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryCeilingBackend {
    /// An operator-delegated cgroup v2 directory whose `memory.max` this process wrote.
    CgroupV2(PathBuf),
    /// The supervisor polls the child's resident size and kills it.
    ResidentWatchdog(ResidentMeasure),
    None,
}

impl MemoryCeilingBackend {
    pub fn name(&self) -> String {
        match self {
            MemoryCeilingBackend::CgroupV2(path) => format!("cgroup-v2:{}", path.display()),
            MemoryCeilingBackend::ResidentWatchdog(measure) => format!("resident-watchdog:{}", measure.name()),
            MemoryCeilingBackend::None => "none".to_string(),
        }
    }
}

/// An operator-delegated cgroup v2 directory the supervisor may write `memory.max` into. Left
/// unset the supervisor falls back to the resident watchdog, which is why a missing cgroup is a
/// degradation and not a refusal.
pub const PALW_WORKER_CGROUP_ENV: &str = "MISAKA_PALW_WORKER_CGROUP";

/// Arm the ceiling for a job. On Linux with a delegated cgroup this writes `memory.max` (the
/// kernel then reclaims and, past that, OOM-kills only inside the cgroup — the node outside it
/// keeps its tip, its peers and its seat). Everywhere else the caller must poll
/// [`resident_bytes`]; [`MemoryCeilingBackend::ResidentWatchdog`] says so.
pub fn arm_memory_ceiling(limit_bytes: u64) -> MemoryCeilingBackend {
    #[cfg(target_os = "linux")]
    {
        if let Ok(dir) = std::env::var(PALW_WORKER_CGROUP_ENV) {
            let dir = PathBuf::from(dir);
            if dir.is_dir() && std::fs::write(dir.join("memory.max"), format!("{limit_bytes}\n")).is_ok() {
                return MemoryCeilingBackend::CgroupV2(dir);
            }
        }
    }
    let (_, measure) = resident_bytes(std::process::id());
    match measure {
        ResidentMeasure::Unavailable => MemoryCeilingBackend::None,
        m => {
            let _ = limit_bytes;
            MemoryCeilingBackend::ResidentWatchdog(m)
        }
    }
}

/// Put a live child into the delegated cgroup, so the kernel charges it. Best effort by design:
/// a failure downgrades to the watchdog rather than failing the job.
pub fn attach_to_cgroup(backend: &MemoryCeilingBackend, pid: u32) -> bool {
    match backend {
        MemoryCeilingBackend::CgroupV2(dir) => std::fs::write(dir.join("cgroup.procs"), format!("{pid}\n")).is_ok(),
        _ => false,
    }
}

// -------------------------------------------------------------------------------------------
// Decision 5 (second half) / Decision 13 / S12 — the confinement backend, reported honestly
// -------------------------------------------------------------------------------------------

/// What is ACTUALLY in force for a spawned worker — never what was configured.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConfinementBackend {
    /// Linux: `seccomp` (no `socket`, no `connect`, no `execve` after setup) plus `Landlock`
    /// (read-only on the artifact and golden paths, write-only on the outbox, nothing else).
    LinuxSeccompLandlock,
    /// macOS: a `sandbox-exec` profile with the same shape.
    MacosSandboxExec,
    /// No platform backend. The environment discipline is still in force; ADR-0079 Decision 10
    /// is what makes this safe, by refusing to let such a host be a public entrance.
    None,
}

impl ConfinementBackend {
    pub fn name(self) -> &'static str {
        match self {
            ConfinementBackend::LinuxSeccompLandlock => "linux-seccomp-landlock",
            ConfinementBackend::MacosSandboxExec => "macos-sandbox-exec",
            ConfinementBackend::None => "none",
        }
    }
}

/// Operator opt-in for a platform backend. Absent (or `none`) leaves the honest default: the
/// environment discipline alone, and ADR-0079 Decision 10 refusing a public entrance.
///
/// Opt-in rather than default because a backend that fails to install must never reach the
/// report, and the only way to know it installed is to PROVE it — see [`establish_confinement`],
/// which declares a backend in force only after its own denials have been observed.
pub const PALW_CONFINEMENT_ENV: &str = "MISAKA_PALW_CONFINEMENT";

/// A confinement that has been installed and PROVEN, or the honest absence of one.
///
/// The value of this type is that it cannot be constructed dishonestly: the only constructor that
/// yields a non-`None` backend is [`establish_confinement`], and that one runs the drill before it
/// returns. A configured-but-broken backend comes back as [`ConfinementBackend::None`] with the
/// reason in the notes.
#[derive(Clone, Debug)]
pub struct Confinement {
    backend: ConfinementBackend,
    /// The generated `sandbox-exec` profile, when the macOS backend is in force.
    profile: Option<PathBuf>,
    /// The resolved Landlock rules and seccomp program, when the Linux backend is in force.
    /// Everything that allocates lives here so the child between `fork` and `execve` allocates
    /// nothing (see `host_security_linux.rs`).
    #[cfg(target_os = "linux")]
    plan: Option<std::sync::Arc<linux::Plan>>,
}

impl Confinement {
    /// The honest absence of a backend.
    pub fn none() -> Self {
        Self {
            backend: ConfinementBackend::None,
            profile: None,
            #[cfg(target_os = "linux")]
            plan: None,
        }
    }

    /// The Linux backend, PROVEN. Private, and called by [`linux::establish`] and nothing else:
    /// the whole value of this type is that it cannot be constructed without the drill.
    #[cfg(target_os = "linux")]
    fn linux_seccomp_landlock(plan: std::sync::Arc<linux::Plan>) -> Self {
        Self { backend: ConfinementBackend::LinuxSeccompLandlock, profile: None, plan: Some(plan) }
    }

    #[cfg(target_os = "linux")]
    fn linux_plan(&self) -> Option<&std::sync::Arc<linux::Plan>> {
        match self.backend {
            ConfinementBackend::LinuxSeccompLandlock => self.plan.as_ref(),
            _ => None,
        }
    }

    pub fn backend(&self) -> ConfinementBackend {
        self.backend
    }

    /// **Build the command for a confined child.** With a backend in force the program is run
    /// under the platform's own mechanism; without one this is `Command::new(program)` and
    /// nothing is pretended. Callers add their arguments and then
    /// [`harden_worker_command`] — the environment discipline applies either way.
    pub fn command(&self, program: &Path) -> Command {
        match (&self.backend, &self.profile) {
            (ConfinementBackend::MacosSandboxExec, Some(profile)) => {
                let mut cmd = Command::new(MACOS_SANDBOX_EXEC);
                cmd.arg("-f").arg(profile).arg(program);
                cmd
            }
            _ => {
                // Linux confines the child itself, in `pre_exec`, rather than wrapping the program
                // in a helper: there is no `sandbox-exec` to hand the profile to, and a wrapper
                // would be one more binary in the execution path.
                #[cfg(target_os = "linux")]
                if let Some(plan) = self.linux_plan() {
                    return linux::confined_command(plan, program);
                }
                Command::new(program)
            }
        }
    }
}

const MACOS_SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// **Install a platform backend and PROVE it before declaring it** (ADR-0079 Decision 5, S3, S12).
///
/// Returns the confinement and a log of what was attempted — every line of which is safe to print,
/// because none of it is key material or a prompt. A backend is declared in force only when its
/// drill has observed the denials it promises; otherwise the result is `none`, honestly, and the
/// log says why. That is S12: *a test that disables the backend asserts the report says `none`
/// rather than the configured value.*
///
/// `writable` is the set of directories the child may write — the outbox, and the working
/// directory. Everything else is read-only or denied.
pub fn establish_confinement(workdir: &Path, writable: &[PathBuf]) -> (Confinement, Vec<String>) {
    let requested = std::env::var(PALW_CONFINEMENT_ENV).unwrap_or_default();
    let requested = requested.trim();
    if requested.is_empty() || requested == "none" {
        return (
            Confinement::none(),
            vec![format!("no backend requested ({PALW_CONFINEMENT_ENV} unset); environment discipline only")],
        );
    }
    #[cfg(target_os = "macos")]
    if requested == ConfinementBackend::MacosSandboxExec.name() {
        return macos::establish(workdir, writable);
    }
    #[cfg(target_os = "linux")]
    if requested == ConfinementBackend::LinuxSeccompLandlock.name() {
        return linux::establish(workdir, writable);
    }
    let _ = (workdir, writable);
    (
        Confinement::none(),
        vec![format!("{PALW_CONFINEMENT_ENV}={requested} is not a backend this build can install on this platform; reporting `none`")],
    )
}

/// The macOS `sandbox-exec` backend (ADR-0079 Decision 5's "per platform, and named honestly when
/// absent" half).
///
/// **What it delivers, stated exactly, because a partial control described as a full one is worse
/// than none:** no network egress, and no writes outside the named directories. **What it does NOT
/// deliver:** a narrowed READ set. The platform's loader needs to read a set this code cannot
/// enumerate — a profile with `file-read*` restricted to the artifact paths aborts every child
/// before `main`, measured on macOS 25.4 — so reads are broad and the report says so. The
/// confidentiality half of Decision 5 is therefore only partly held here, which is what "macOS
/// gets a partial one" means in ADR-0079 §4.
#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    fn sbpl_path(path: &Path) -> String {
        // sandbox subpath matching is on the REAL path: `/var` is a symlink to `/private/var`, and
        // a profile that names the symlink denies every write under it. Measured, not assumed.
        let real = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        real.display().to_string().replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn write_profile(workdir: &Path, writable: &[PathBuf]) -> Result<PathBuf, String> {
        let mut writes = String::new();
        for dir in writable {
            writes.push_str(&format!("\n    (subpath \"{}\")", sbpl_path(dir)));
        }
        let profile = format!(
            "(version 1)\n\
             ;; ADR-0079 Decision 5 — the capability set is the arithmetic's, deny-by-default.\n\
             (deny default)\n\
             (allow process*)\n\
             (allow sysctl-read)\n\
             (allow mach*)\n\
             (allow signal)\n\
             ;; Reads are BROAD and this is the backend's stated limitation: a read set narrowed to\n\
             ;; the artifact paths aborts every child before main on this platform.\n\
             (allow file-read*)\n\
             ;; Writes are the outbox and the working directory, and nothing else.\n\
             (allow file-write* (subpath \"{}\"){writes})\n\
             ;; The determinism rules already forbid a network read; here the OS enforces it.\n\
             (deny network*)\n",
            sbpl_path(workdir)
        );
        let path = workdir.join("palw-worker.sb");
        let mut file = std::fs::File::create(&path).map_err(|e| format!("cannot write the sandbox profile: {e}"))?;
        file.write_all(profile.as_bytes()).map_err(|e| format!("cannot write the sandbox profile: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(path)
    }

    fn under(profile: &Path, program: &str, args: &[&str]) -> Option<std::process::ExitStatus> {
        Command::new(MACOS_SANDBOX_EXEC)
            .arg("-f")
            .arg(profile)
            .arg(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
    }

    /// **The drill** (S3): each denial is EXERCISED and the denial asserted — never the absence of
    /// a crash. The network probe is differential and entirely local: a listener this process owns
    /// is reachable without the sandbox and unreachable with it, so a refused connection cannot be
    /// mistaken for a refused *port*.
    pub(super) fn establish(workdir: &Path, writable: &[PathBuf]) -> (Confinement, Vec<String>) {
        let mut notes = Vec::new();
        if !Path::new(MACOS_SANDBOX_EXEC).is_file() {
            notes.push(format!("{MACOS_SANDBOX_EXEC} is not on this host; reporting `none`"));
            return (Confinement::none(), notes);
        }
        let profile = match write_profile(workdir, writable) {
            Ok(path) => path,
            Err(e) => {
                notes.push(format!("{e}; reporting `none`"));
                return (Confinement::none(), notes);
            }
        };

        // (a) a child can start at all.
        match under(&profile, "/bin/echo", &["ok"]) {
            Some(status) if status.success() => notes.push("drill: a child starts under the profile".into()),
            _ => {
                notes.push("drill FAILED: no child starts under the profile; reporting `none`".into());
                return (Confinement::none(), notes);
            }
        }

        // (b) a write inside the allowed set succeeds, and one outside is DENIED.
        let inside = workdir.join("palw-confinement-probe");
        let outside = std::env::temp_dir().join("palw-confinement-probe-outside");
        let _ = std::fs::remove_file(&inside);
        let _ = std::fs::remove_file(&outside);
        let wrote_inside =
            under(&profile, "/bin/sh", &["-c", &format!("echo probe > {}", inside.display())]).is_some_and(|s| s.success());
        let wrote_outside =
            under(&profile, "/bin/sh", &["-c", &format!("echo probe > {}", outside.display())]).is_some_and(|s| s.success());
        let _ = std::fs::remove_file(&inside);
        let _ = std::fs::remove_file(&outside);
        if !wrote_inside || wrote_outside {
            notes.push(format!(
                "drill FAILED: write inside the allowed set {} and write outside {}; reporting `none`",
                if wrote_inside { "succeeded" } else { "was DENIED (it must not be)" },
                if wrote_outside { "succeeded (it must not)" } else { "was denied" }
            ));
            return (Confinement::none(), notes);
        }
        notes.push("drill: writes land inside the outbox and are denied outside it".into());

        // (c) the network denial, proven against a listener this process owns.
        let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
            notes.push("drill FAILED: cannot open a local listener to prove the network denial; reporting `none`".into());
            return (Confinement::none(), notes);
        };
        let port = match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(_) => {
                notes.push("drill FAILED: the probe listener has no address; reporting `none`".into());
                return (Confinement::none(), notes);
            }
        };
        if !Path::new("/usr/bin/nc").is_file() {
            notes.push("drill FAILED: /usr/bin/nc is absent, so the network denial cannot be PROVEN; reporting `none`".into());
            return (Confinement::none(), notes);
        }
        let port = port.to_string();
        let unconfined = Command::new("/usr/bin/nc")
            .args(["-z", "-w", "2", "127.0.0.1", &port])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        let confined = under(&profile, "/usr/bin/nc", &["-z", "-w", "2", "127.0.0.1", &port]).is_some_and(|s| s.success());
        drop(listener);
        if !unconfined {
            notes.push("drill INCONCLUSIVE: the probe listener was unreachable even unconfined; reporting `none`".into());
            return (Confinement::none(), notes);
        }
        if confined {
            notes.push("drill FAILED: the sandboxed child reached a socket; reporting `none`".into());
            return (Confinement::none(), notes);
        }
        notes.push("drill: a socket reachable unconfined is DENIED under the profile".into());
        notes.push(
            "limitation, stated: reads are broad under this backend — a read set narrowed to the artifact paths aborts \
             every child before main on this platform. What it delivers is no network and no writes outside two directories."
                .into(),
        );

        declare_backend_in_force(ConfinementBackend::MacosSandboxExec);
        (Confinement { backend: ConfinementBackend::MacosSandboxExec, profile: Some(profile) }, notes)
    }
}

static BACKEND_IN_FORCE: OnceLock<ConfinementBackend> = OnceLock::new();

/// Record that a backend was successfully installed for children of this process. Called by the
/// backend's own installer and by nothing else: a configured backend that failed to install must
/// never reach the report, which is the whole of S12.
pub fn declare_backend_in_force(backend: ConfinementBackend) {
    let _ = BACKEND_IN_FORCE.set(backend);
}

/// The backend actually in force. `None` until an installer says otherwise — the honest default,
/// because no installer has shipped for this platform yet (ADR-0079 R-04).
pub fn confinement_backend_in_force() -> ConfinementBackend {
    *BACKEND_IN_FORCE.get().unwrap_or(&ConfinementBackend::None)
}

/// The backend this build COULD install on this host — a different question from what is in
/// force, reported as a different field, and never a substitute for it. A host where this says
/// `macos-sandbox-exec` and [`confinement_backend_in_force`] says `none` is a host where nobody
/// asked for it, or where its drill did not pass.
///
/// On Linux the answer is read from the kernel, not from a version string: the LSM list
/// (`/sys/kernel/security/lsm`) must name `landlock`, `landlock_create_ruleset` must answer with
/// an ABI level, and `prctl(PR_GET_SECCOMP)` must answer at all. A kernel that was built with
/// Landlock and booted without it says so in the first of those, and a version number would not.
pub fn confinement_backend_available() -> ConfinementBackend {
    #[cfg(target_os = "macos")]
    {
        if Path::new(MACOS_SANDBOX_EXEC).is_file() {
            return ConfinementBackend::MacosSandboxExec;
        }
    }
    #[cfg(target_os = "linux")]
    {
        if linux::kernel_supports_backend().is_ok() {
            return ConfinementBackend::LinuxSeccompLandlock;
        }
    }
    ConfinementBackend::None
}

/// **Decision 5's "no `execve` after setup", stacked by the process it applies to.**
///
/// The supervisor installs its filter between `fork` and `execve`, so that filter cannot deny the
/// worker's own exec. Seccomp filters STACK and cannot be removed, so the worker denies `execve`
/// on itself, once, at the top of `main`:
///
/// ```no_run
/// // first line of a worker binary's main():
/// let _ = misaka_palw::host_security::confine_self_after_exec();
/// ```
///
/// It stacks the denial only when a filter is ALREADY in force — i.e. when a confining supervisor
/// spawned this process. A worker started by hand keeps every capability it had, because a binary
/// whose posture depended on who ran it would make the report an assumption. The return value says
/// which of those happened; nothing here ever silently claims more than it did.
pub fn confine_self_after_exec() -> ExecveDenial {
    #[cfg(target_os = "linux")]
    {
        linux::confine_self_after_exec()
    }
    #[cfg(not(target_os = "linux"))]
    {
        ExecveDenial::Unsupported("no seccomp on this platform; the execve denial is a Linux-only control".to_string())
    }
}

/// What [`confine_self_after_exec`] did — reported, never assumed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecveDenial {
    /// A supervisor's filter was in force and the `execve` denial is now stacked on it.
    Stacked,
    /// No filter was in force: this process was not spawned by a confining supervisor, so nothing
    /// was stacked.
    NotConfined,
    /// This platform or kernel has no seccomp filter mode.
    Unsupported(String),
    /// A filter WAS in force and the stack failed. The caller should refuse to run: the process
    /// has less confinement than the supervisor's report already claims.
    Failed(String),
}

impl ExecveDenial {
    pub fn name(&self) -> &'static str {
        match self {
            ExecveDenial::Stacked => "execve-denied",
            ExecveDenial::NotConfined => "not-confined",
            ExecveDenial::Unsupported(_) => "unsupported",
            ExecveDenial::Failed(_) => "failed",
        }
    }
}

// -------------------------------------------------------------------------------------------
// Decision 10 / S6 — the public-entrance guard, shared so there is ONE spelling of the rule
// -------------------------------------------------------------------------------------------

/// The acknowledgement variable for a non-loopback PALW gateway bind (ADR-0079 Decision 10),
/// extending `SECURITY.md`'s existing `RKSTRATUM_ALLOW_PUBLIC_DASHBOARD` pattern rather than
/// inventing a second one.
pub const ALLOW_PUBLIC_GATEWAY_ENV: &str = "MISAKA_PALW_ALLOW_PUBLIC_GATEWAY";

/// Is this listen address a loopback bind? A host name that is not an IP literal is treated as
/// public: a name resolves to whatever DNS says today, which is not a property a startup guard
/// may take on trust.
pub fn listen_is_loopback(listen: &str) -> bool {
    let host = match listen.rfind(':') {
        Some(at) => &listen[..at],
        None => listen,
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => host.eq_ignore_ascii_case("localhost"),
    }
}

/// Decision 10's two refusals, as one function so the gateway and the report cannot disagree
/// about what the rule is. `Ok(())` means the bind may proceed.
pub fn check_public_bind(listen: &str, acknowledged: bool, backend: ConfinementBackend) -> Result<(), String> {
    if listen_is_loopback(listen) {
        return Ok(());
    }
    if !acknowledged {
        return Err(format!(
            "refusing to bind {listen}: this is a PUBLIC entrance and a stranger chooses its input.\n\
             The intended production pattern is an AUTHENTICATING REVERSE PROXY in front of a \
             loopback-bound gateway (bind 127.0.0.1 and proxy to it).\n\
             If you have that proxy — or you accept the exposure — acknowledge it explicitly with \
             {ALLOW_PUBLIC_GATEWAY_ENV}=1."
        ));
    }
    if backend == ConfinementBackend::None {
        return Err(format!(
            "refusing to bind {listen}: the confinement backend in force on this host is `none`, and a public \
             entrance is the one place where a stranger chooses the model's input (ADR-0079 Decision 10).\n\
             {ALLOW_PUBLIC_GATEWAY_ENV}=1 does NOT override this. Bind loopback and put an authenticating \
             reverse proxy in front of it."
        ));
    }
    Ok(())
}

/// Is the public-gateway acknowledgement set in this process's environment?
pub fn public_gateway_acknowledged() -> bool {
    std::env::var(ALLOW_PUBLIC_GATEWAY_ENV).map(|v| v == "1").unwrap_or(false)
}

// -------------------------------------------------------------------------------------------
// Decision 4 / S5 — no process that parses a stranger's bytes holds a key
// -------------------------------------------------------------------------------------------

/// Environment names that would put a signing secret in a process's view. A gateway that finds
/// any of them set refuses to boot: it holds the executor PUBLIC key and nothing else, and the
/// ML-DSA signature belongs to the signer sidecar.
pub const SIGNING_SECRET_ENV_NAMES: &[&str] = &[
    "MISAKA_BOND_KEY_SEED",
    "MISAKA_PALW_BOND_KEY_SEED",
    "MISAKA_VALIDATOR_SEED",
    "KASPA_PQ_VALIDATOR_SEED",
    "KASPA_PQ_SIGNER_SEED",
    "MISAKA_WALLET_SEED",
    "MISAKA_KEY_SEED",
];

/// The byte length of a raw ML-DSA-87 keygen seed. A file of exactly this size sitting in a
/// directory the gateway reads is the shape of the mistake this check exists to catch.
pub const VALIDATOR_SEED_FILE_LEN: u64 = 32;

/// A signing secret found in a process's own view, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReachableSecret {
    Environment(String),
    SeedShapedFile(PathBuf),
}

impl std::fmt::Display for ReachableSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReachableSecret::Environment(name) => write!(f, "the environment variable {name} is set"),
            ReachableSecret::SeedShapedFile(path) => {
                write!(f, "{} is a {VALIDATOR_SEED_FILE_LEN}-byte file — the shape of a raw ML-DSA-87 keygen seed", path.display())
            }
        }
    }
}

/// Everything secret-shaped that a public-facing process can reach without looking hard: its own
/// environment, and the directories it was pointed at. Not a proof of absence — a process that
/// can read the filesystem can always find more — but it catches the deployment where the bond
/// seed was dropped next to the identity file "for now".
pub fn reachable_signing_secrets<F>(lookup: F, dirs: &[&Path]) -> Vec<ReachableSecret>
where
    F: Fn(&str) -> Option<String>,
{
    let mut found = Vec::new();
    for name in SIGNING_SECRET_ENV_NAMES {
        if lookup(name).is_some_and(|v| !v.trim().is_empty()) {
            found.push(ReachableSecret::Environment((*name).to_string()));
        }
    }
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_file() && meta.len() == VALIDATOR_SEED_FILE_LEN {
                found.push(ReachableSecret::SeedShapedFile(entry.path()));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **S2, the constant half.** The delivered set EQUALS the allowlist ∩ parent plus the pinned
    /// locale — it does not merely contain it. Everything the operator's shell carries stops here.
    #[test]
    fn the_delivered_environment_equals_the_constant() {
        let parent: BTreeMap<&str, &str> = [
            ("MISAKA_PALW_GGUF", "/srv/models/qwen.gguf"),
            ("MISAKA_PALW_GOLDEN", "/srv/golden.borsh"),
            // Everything below is what the finding is about.
            ("SSH_AUTH_SOCK", "/private/tmp/ssh-agent.sock"),
            ("AWS_SECRET_ACCESS_KEY", "not-a-real-key"),
            ("PATH", "/usr/local/bin:/usr/bin:/bin"),
            ("HOME", "/Users/operator"),
            ("MISAKA_WALLET_SEED", "/Users/operator/.misaka/seed"),
            ("LC_ALL", "en_US.UTF-8"),
        ]
        .into_iter()
        .collect();

        let delivered = worker_environment_from(|name| parent.get(name).map(|v| (*v).to_string()));

        let expected: BTreeMap<String, String> = [
            ("LANG", "C"),
            ("LC_ALL", "C"),
            ("LC_NUMERIC", "C"),
            ("MISAKA_PALW_GGUF", "/srv/models/qwen.gguf"),
            ("MISAKA_PALW_GOLDEN", "/srv/golden.borsh"),
            ("TZ", "UTC"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(delivered.vars, expected, "the delivered environment must EQUAL the constant, not contain it");

        // Named individually, because a future edit that re-adds one of these should fail on the
        // line that says why rather than on a map diff.
        for forbidden in ["SSH_AUTH_SOCK", "AWS_SECRET_ACCESS_KEY", "HOME", "MISAKA_WALLET_SEED"] {
            assert!(!delivered.vars.contains_key(forbidden), "{forbidden} must not reach the model process");
        }
        // SA-4: the supervisor spawns by absolute path, so an inherited PATH is only an execution
        // vector. This assertion is the amendment.
        assert!(!delivered.vars.contains_key("PATH"), "PATH left the allowlist in ADR-0079 SA-4 and must not come back");
        // The pin beats the inherited value rather than passing it through.
        assert_eq!(delivered.vars.get("LC_ALL").map(String::as_str), Some("C"));
    }

    /// The allowlist is a constant in the tree, and every name on it is a `MISAKA_PALW_*` name a
    /// shipped worker reads. A name that is neither is an ambient capability with a nicer label.
    #[test]
    fn the_allowlist_is_narrow_and_named() {
        assert!(!PALW_WORKER_ENV_ALLOWLIST.is_empty());
        for name in PALW_WORKER_ENV_ALLOWLIST {
            assert!(name.starts_with("MISAKA_PALW_"), "{name} is not a PALW artifact variable");
        }
        assert!(!PALW_WORKER_ENV_ALLOWLIST.contains(&"PATH"));
        assert!(!PALW_WORKER_ENV_ALLOWLIST.contains(&"HOME"));
        assert!(!PALW_WORKER_ENV_ALLOWLIST.contains(&"LD_PRELOAD"));
        assert!(!PALW_WORKER_ENV_ALLOWLIST.contains(&"DYLD_INSERT_LIBRARIES"));
        // Every name a shipped worker refuses to start without must be here, or the worker is
        // unstartable through a gateway and the gateway can only report that it "exited". The A16
        // free-prompt worker refuses without its network id, which is an input to the context hash
        // the court re-derives — not a capability.
        assert!(PALW_WORKER_ENV_ALLOWLIST.contains(&"MISAKA_PALW_NETWORK_ID"));
    }

    /// **S2, the delivered half.** Spawn a real child through the real hardening and compare what
    /// it PRINTS. A constant a test reads and a spawn a supervisor performs are two different
    /// things, and only the second one is the finding.
    #[cfg(unix)]
    #[test]
    fn a_hardened_child_receives_exactly_the_delivered_set() {
        let env_bin = ["/usr/bin/env", "/bin/env"].iter().map(PathBuf::from).find(|p| p.is_file());
        let Some(env_bin) = env_bin else {
            eprintln!("no /usr/bin/env on this host; the constant half of S2 still ran");
            return;
        };
        // SAFETY: single-threaded test setup, before any spawn.
        unsafe {
            std::env::set_var("SSH_AUTH_SOCK", "/private/tmp/should-not-be-inherited");
            std::env::set_var("MISAKA_PALW_GGUF", "/srv/models/pinned.gguf");
        }
        let workdir = std::env::temp_dir();
        let mut cmd = Command::new(&env_bin);
        let delivered = harden_worker_command(&mut cmd, &workdir);
        let out = cmd.output().expect("spawn env");
        assert!(out.status.success(), "env exited {}", out.status);
        let mut got: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
        got.sort();
        let mut want = delivered.as_env_lines();
        want.sort();
        assert_eq!(got, want, "the child's ACTUAL environment must equal the delivered set exactly");
        assert!(!got.iter().any(|l| l.starts_with("SSH_AUTH_SOCK=")), "the agent socket reached the child");
        assert!(!got.iter().any(|l| l.starts_with("PATH=")), "PATH reached the child");
    }

    #[test]
    fn loopback_is_recognised_and_a_name_is_not_taken_on_trust() {
        for ok in ["127.0.0.1:8790", "127.0.0.1", "[::1]:8790", "localhost:8790"] {
            assert!(listen_is_loopback(ok), "{ok} is loopback");
        }
        for public in ["0.0.0.0:8790", "192.168.1.10:8790", "[::]:8790", "gateway.example.com:8790"] {
            assert!(!listen_is_loopback(public), "{public} is not loopback");
        }
    }

    /// **S6.** A public bind needs the acknowledgement, AND fails unconditionally when the
    /// backend in force is `none` — the acknowledgement does not override the second rule.
    #[test]
    fn a_public_bind_needs_the_acknowledgement_and_a_backend() {
        assert!(check_public_bind("127.0.0.1:8790", false, ConfinementBackend::None).is_ok());

        let err = check_public_bind("0.0.0.0:8790", false, ConfinementBackend::LinuxSeccompLandlock).unwrap_err();
        assert!(err.contains(ALLOW_PUBLIC_GATEWAY_ENV), "the failure must name the acknowledgement");
        assert!(err.to_lowercase().contains("reverse proxy"), "the failure must name the intended pattern");

        assert!(check_public_bind("0.0.0.0:8790", true, ConfinementBackend::LinuxSeccompLandlock).is_ok());

        let err = check_public_bind("0.0.0.0:8790", true, ConfinementBackend::None).unwrap_err();
        assert!(err.contains("none"), "the failure must say which backend is in force");
        assert!(err.contains("does NOT override"), "the acknowledgement must not be a way past this one");
    }

    /// **S12, the default.** The honest absence of a backend is a value with a name, and
    /// `available` may only ever name THIS platform's installer.
    ///
    /// This deliberately does NOT assert the process-global `confinement_backend_in_force()`: the
    /// drills below declare it when they pass, and a test that read that global would pass or fail
    /// on test ORDER. The real S12 assertion — a backend that was configured but did not install
    /// reports `none` — lives in [`an_unrequested_or_unsupported_backend_reports_none`], on the
    /// returned value.
    #[test]
    fn the_absence_of_a_backend_is_a_named_value() {
        assert_eq!(Confinement::none().backend(), ConfinementBackend::None);
        assert_eq!(ConfinementBackend::None.name(), "none");
        // "available" is not "in force": on a host that HAS the mechanism this build COULD install
        // one, and that says nothing about whether anyone asked or whether its drill passed. What
        // it may never do is name another platform's backend.
        let available = confinement_backend_available();
        let allowed: &[ConfinementBackend] = if cfg!(target_os = "macos") {
            &[ConfinementBackend::None, ConfinementBackend::MacosSandboxExec]
        } else if cfg!(target_os = "linux") {
            &[ConfinementBackend::None, ConfinementBackend::LinuxSeccompLandlock]
        } else {
            &[ConfinementBackend::None]
        };
        assert!(allowed.contains(&available), "{} is not a backend this platform can install", available.name());
    }

    /// **S5.** A secret in the process's own view is found by name and by shape.
    #[test]
    fn reachable_secrets_are_found_by_name_and_by_shape() {
        let dir = std::env::temp_dir().join(format!("misaka-hostsec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let seed = dir.join("bond.seed");
        std::fs::write(&seed, [7u8; 32]).unwrap();
        std::fs::write(dir.join("identity.json"), b"{\"not\":\"a seed\"}").unwrap();

        let found = reachable_signing_secrets(
            |name| if name == "MISAKA_BOND_KEY_SEED" { Some("/srv/bond.seed".into()) } else { None },
            &[dir.as_path()],
        );
        assert!(found.contains(&ReachableSecret::Environment("MISAKA_BOND_KEY_SEED".into())));
        assert!(found.contains(&ReachableSecret::SeedShapedFile(seed)));
        assert_eq!(found.len(), 2, "the JSON beside it is not seed-shaped and must not be reported");

        let clean = reachable_signing_secrets(|_| None, &[]);
        assert!(clean.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **S12, the constructive half.** A backend nobody requested is `none`, and a backend name
    /// this build cannot install is `none` too — with the reason, never the configured value.
    #[test]
    fn an_unrequested_or_unsupported_backend_reports_none() {
        let workdir = std::env::temp_dir();
        // SAFETY: single-threaded test setup.
        unsafe { std::env::remove_var(PALW_CONFINEMENT_ENV) };
        let (conf, notes) = establish_confinement(&workdir, &[]);
        assert_eq!(conf.backend(), ConfinementBackend::None);
        assert!(notes.iter().any(|n| n.contains("no backend requested")), "{notes:?}");

        // The OTHER platform's backend: a real name this build can spell and cannot install here.
        let foreign = if cfg!(target_os = "linux") {
            ConfinementBackend::MacosSandboxExec.name()
        } else {
            ConfinementBackend::LinuxSeccompLandlock.name()
        };
        unsafe { std::env::set_var(PALW_CONFINEMENT_ENV, foreign) };
        let (conf, notes) = establish_confinement(&workdir, &[]);
        assert_eq!(conf.backend(), ConfinementBackend::None, "a backend this build cannot install is `none`, not a promise");
        assert!(notes.iter().any(|n| n.contains("not a backend this build can install")), "{notes:?}");
        unsafe { std::env::remove_var(PALW_CONFINEMENT_ENV) };
    }

    /// **S3 and S4 on this platform.** The macOS backend is installed only after its own drill has
    /// OBSERVED the denials: a child starts, a write lands inside the allowed set, a write outside
    /// is denied, and a socket that is reachable unconfined is denied under the profile. And the
    /// child's own output is unchanged by the confinement — S4's "identical roots or the job
    /// fails", in the only form a test without a model can assert.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_macos_backend_proves_its_denials_before_it_is_declared() {
        let workdir = std::env::temp_dir().join(format!("palw-sbx-{}", std::process::id()));
        std::fs::create_dir_all(&workdir).unwrap();
        // SAFETY: single-threaded test setup.
        unsafe { std::env::set_var(PALW_CONFINEMENT_ENV, ConfinementBackend::MacosSandboxExec.name()) };
        let (conf, notes) = establish_confinement(&workdir, std::slice::from_ref(&workdir));
        unsafe { std::env::remove_var(PALW_CONFINEMENT_ENV) };

        if conf.backend() == ConfinementBackend::None {
            // A host where the drill could not run (no nc, no sandbox-exec, a restricted CI) is
            // reported as `none` — which is the property, not a skipped test.
            assert!(notes.iter().any(|n| n.contains("`none`")), "a refusal must say why: {notes:?}");
            return;
        }
        assert_eq!(conf.backend(), ConfinementBackend::MacosSandboxExec);
        assert!(notes.iter().any(|n| n.contains("DENIED under the profile")), "the network denial must be observed: {notes:?}");
        assert!(notes.iter().any(|n| n.contains("denied outside it")), "the write denial must be observed: {notes:?}");
        assert!(notes.iter().any(|n| n.contains("limitation, stated")), "the partial backend must name its gap: {notes:?}");

        // S4's shape: the same deterministic child produces the same bytes confined and not.
        let plain = Command::new("/bin/echo").arg("deterministic").output().expect("echo");
        let confined = conf.command(Path::new("/bin/echo")).arg("deterministic").output().expect("echo under the sandbox");
        assert_eq!(plain.stdout, confined.stdout, "confinement must not change what a child computes");
        assert_eq!(plain.status.success(), confined.status.success());
        std::fs::remove_dir_all(&workdir).ok();
    }

    /// **S3 and S4 on Linux.** The `seccomp` + `Landlock` backend is installed only after its own
    /// drill has OBSERVED the denials: a dynamically linked child starts, a write lands inside the
    /// writable set and is denied outside it, a file readable UNCONFINED is denied under the
    /// profile — the read half macOS could not deliver — a socket reachable unconfined is refused,
    /// and a child that stacks the `execve` denial can no longer exec.
    ///
    /// And S4's shape: the same deterministic child produces identical bytes confined and
    /// unconfined. That is the invariant that lets the backend ship at all — a security control
    /// that could change an arithmetic result is a fork risk.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_linux_backend_proves_its_denials_before_it_is_declared() {
        let workdir = std::env::temp_dir().join(format!("palw-ll-{}", std::process::id()));
        std::fs::create_dir_all(&workdir).unwrap();
        // SAFETY: single-threaded test setup.
        unsafe { std::env::set_var(PALW_CONFINEMENT_ENV, ConfinementBackend::LinuxSeccompLandlock.name()) };
        let (conf, notes) = establish_confinement(&workdir, std::slice::from_ref(&workdir));
        unsafe { std::env::remove_var(PALW_CONFINEMENT_ENV) };

        if conf.backend() == ConfinementBackend::None {
            // A kernel with no Landlock, a container that hides the LSM list, a restricted CI —
            // all report `none` WITH THE REASON, which is the property rather than a skipped test.
            assert!(notes.iter().any(|n| n.contains("`none`")), "a refusal must say why: {notes:?}");
            // But on a host whose kernel DOES expose the mechanism, `none` may only ever be an
            // INCONCLUSIVE drill — a missing probe binary, a temp dir that overlaps the writable
            // set. A FAILED leg there is a profile that does not do what this file says it does,
            // and this assertion is what stops that from passing as a skipped test.
            if confinement_backend_available() == ConfinementBackend::LinuxSeccompLandlock {
                assert!(
                    !notes.iter().any(|n| n.contains("drill FAILED")),
                    "this kernel exposes Landlock and seccomp, and a drill leg FAILED: {notes:?}"
                );
            }
            std::fs::remove_dir_all(&workdir).ok();
            return;
        }
        assert_eq!(conf.backend(), ConfinementBackend::LinuxSeccompLandlock);
        for observed in [
            "starts under the profile",
            "denied outside it",
            "readable UNCONFINED and DENIED under the profile",
            "a read inside the allowed set is permitted",
            "a socket reachable unconfined is DENIED under the profile",
            "cannot exec",
            "limitation, stated",
        ] {
            assert!(notes.iter().any(|n| n.contains(observed)), "the drill must observe {observed:?}: {notes:?}");
        }

        // S4: confinement changes what a child MAY do, never what it computes.
        let plain = Command::new("/bin/echo").arg("deterministic").output().expect("echo");
        let confined = conf.command(Path::new("/bin/echo")).arg("deterministic").output().expect("echo under the profile");
        assert_eq!(plain.stdout, confined.stdout, "confinement must not change what a child computes");
        assert_eq!(plain.status.success(), confined.status.success());
        std::fs::remove_dir_all(&workdir).ok();
    }

    /// The worker's own half of Decision 5 stacks a denial only on a process a confining
    /// supervisor spawned. A binary that confined itself regardless would have a posture that
    /// depends on who ran it, and the report would then be reading an assumption.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_unsupervised_process_stacks_no_execve_denial() {
        // 21 is `PR_GET_SECCOMP`. The assertion below is only meaningful — and only SAFE — on a
        // process that is not already filtered: calling the function under a filter would install
        // one on the test binary and break every later spawn in this process.
        if unsafe { libc::prctl(21) } != 0 {
            eprintln!("this test process is already seccomp-filtered; the NotConfined case cannot be observed here");
            return;
        }
        assert_eq!(confine_self_after_exec(), ExecveDenial::NotConfined);
        assert_eq!(ExecveDenial::NotConfined.name(), "not-confined");
        assert_eq!(ExecveDenial::Stacked.name(), "execve-denied");
    }

    /// The ceiling is named for what it measures and is above the hybrid class's mapped artifact,
    /// because SA-1's whole point is that an address-space-shaped cap kills the worker at startup.
    #[test]
    fn the_resident_ceiling_clears_the_hybrid_artifact() {
        assert!(PALW_WORKER_MAX_RESIDENT_BYTES > 33 * 1024 * 1024 * 1024, "the hybrid maps 33 GiB (ADR-0079 SA-1)");
    }
}
