//! **The host ledger: every kaspad on one host reserves against the host's memory TOGETHER**
//! (int-10.2 D1; `docs/design/palw/t12-replay-memory-1001.md` §5).
//!
//! # The failure a per-process ledger cannot see
//!
//! Each node's memory ledger (`palw_memory_ledger`) grants a duty when its need fits the node's own
//! share AND the host's headroom now — `MemAvailable − 1 GiB`, less what THIS node has reserved. Five
//! seats on one host each read the same `MemAvailable`, each subtract only their own reservations, and
//! each start: on 5.104 (24 GiB, five seats of 3.5 GiB share, 2026-10-01) the shares alone were 17.5 GiB
//! beside five live sets of ~3 GiB, the host swapped all night, and replays took 7–50 minutes. Lane P
//! put it plainly: per-seat cgroups cannot fix a host that is over-committed in aggregate — five caps
//! that each fit the host sum past it. Only a ledger the seats SHARE can say "the host is full".
//!
//! # What this is
//!
//! A small text file in a directory every process on the host names (`--palw-host-ledger-dir`; a tmpfs
//! such as `/run/misaka-palw` — the file describes live processes and nothing else), guarded by an
//! exclusive `flock(2)` on a lock file beside it and rewritten whole by rename:
//!
//! ```text
//! n <pid> <start> <instance> <kind>                                    a process on the host: node | pinner
//! r <pid> <start> <instance> <id> <bytes> <since> <role>               a reservation its ledger granted
//! p <pid> <start> <instance> <dev> <ino> <size> <mtime_s> <mtime_ns>   a class artifact it holds pinned
//! ```
//!
//! A grant then needs BOTH the node's own bound (its share and its live headroom, unchanged) AND the
//! host's aggregate one:
//!
//! ```text
//! need ≤ host_bound − Σ reservations of every live process on the host
//! host_bound = MemAvailable − 1 GiB   (a node with a declared share; 70 % of it without one — the same policy)
//! ```
//!
//! — the node ledger's own rule, with the host's reservations in place of the node's. It keeps the node
//! ledger's deliberate double count (a reservation's touched pages have left `MemAvailable` AND are
//! subtracted again), host-wide: the error is in the direction of holding a duty that would have fitted.
//! Pinned pages are already outside `MemAvailable` (they are unevictable), so they need no term of their
//! own: the `p` lines are the host's arithmetic and its report, each distinct file counted ONCE however
//! many processes lock it. (A seat that starts beside a pinner holding a file needs nothing from them:
//! `mincore` already reads the file resident, so the seat's own pin reads nothing and always fits.) The
//! `n` lines say who takes part — `host_nodes` and `host_pinner` in the status — so an operator sees a
//! seat that does not (an older release, a seat started without the directory).
//!
//! # Liveness, staleness, and failure
//!
//! A line whose process is gone — `kill(pid, 0)` says ESRCH, or (Linux) `/proc/<pid>/stat`'s start time
//! is not the one recorded, a reused pid — is dropped by the next writer. A node that cannot use the
//! directory (absent and not creatable, not writable) runs without the host ledger and says so once: the
//! old per-node behaviour, never a hold. An I/O error at a grant is the same: logged, and the node's own
//! bound decides — failing open returns to int-10.1, failing closed would hold every duty on the host.
//! The file is not fsynced: after a host crash every line in it is a dead process's, and a writer that
//! waited on a disk under swap pressure would hold its node's ledger while it did.
//!
//! **Node-local, never consensus**, and OFF unless the operator names the directory: a release that
//! changes what a node starts must not let one host's file decide it by default (§5 of the note argues
//! the default). Nothing the chain reads depends on it.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use kaspa_core::warn;
use misaka_palw_base0::mmap::FileIdentityV1;

/// The file the ledger lives in, inside the operator's directory.
pub const PALW_HOST_LEDGER_FILE_V1: &str = "host-ledger-v1";
/// The lock file beside it (the data file is replaced by rename, so the lock cannot live on it).
pub const PALW_HOST_LEDGER_LOCK_V1: &str = "host-ledger-v1.lock";
/// A kaspad node (a seat, a producer) on the host — the `n` line's kind.
pub const PALW_HOST_MEMBER_NODE_V1: &str = "node";
/// The host pinner (`kaspad --palw-host-pinner`) — the `n` line's kind.
pub const PALW_HOST_MEMBER_PINNER_V1: &str = "pinner";

/// One process on the host that takes part, and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwHostMemberV1 {
    pub pid: u32,
    pub start: u64,
    pub instance: u64,
    pub kind: String,
}

/// One reservation a process on the host holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwHostReservationV1 {
    pub pid: u32,
    pub start: u64,
    pub instance: u64,
    pub id: u64,
    pub bytes: u64,
    pub role: String,
    pub since_unix: u64,
}

/// One artifact a process on the host holds pinned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwHostPinV1 {
    pub pid: u32,
    pub start: u64,
    pub instance: u64,
    pub identity: FileIdentityV1,
}

/// The file's contents, parsed: who is on the host and what every live process holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwHostLedgerStateV1 {
    pub members: Vec<PalwHostMemberV1>,
    pub reservations: Vec<PalwHostReservationV1>,
    pub pins: Vec<PalwHostPinV1>,
}

impl PalwHostLedgerStateV1 {
    pub fn reserved_bytes(&self) -> u64 {
        self.reservations.iter().fold(0u64, |acc, r| acc.saturating_add(r.bytes))
    }

    /// The distinct pinned files on the host and their bytes — each counted ONCE, however many
    /// processes lock it.
    pub fn distinct_pinned(&self) -> (usize, u64) {
        let mut files: BTreeMap<(u64, u64, u64, i64, i64), u64> = BTreeMap::new();
        for pin in &self.pins {
            let i = pin.identity;
            files.insert((i.dev, i.ino, i.len, i.mtime_sec, i.mtime_nsec), i.len);
        }
        (files.len(), files.values().fold(0u64, |acc, b| acc.saturating_add(*b)))
    }

    /// The node ledgers holding reservations now (two ledgers of one process are two: a test runs several).
    pub fn reserving(&self) -> usize {
        let mut holders: Vec<(u32, u64, u64)> = self.reservations.iter().map(|r| (r.pid, r.start, r.instance)).collect();
        holders.sort_unstable();
        holders.dedup();
        holders.len()
    }

    /// The live processes of one kind that registered themselves.
    pub fn members_of(&self, kind: &str) -> usize {
        self.members.iter().filter(|m| m.kind == kind).count()
    }

    /// Drop every line of a process that is gone; whether any was.
    fn prune_dead(&mut self) -> bool {
        let before = (self.members.len(), self.reservations.len(), self.pins.len());
        self.members.retain(|m| process_is_v1(m.pid, m.start));
        self.reservations.retain(|r| process_is_v1(r.pid, r.start));
        self.pins.retain(|p| process_is_v1(p.pid, p.start));
        before != (self.members.len(), self.reservations.len(), self.pins.len())
    }

    fn encode(&self) -> String {
        let mut out = String::from("# misaka PALW host ledger v1 (int-10.2 D1) — rewritten whole under host-ledger-v1.lock\n");
        for m in &self.members {
            out.push_str(&format!("n {} {} {} {}\n", m.pid, m.start, m.instance, sanitize(&m.kind)));
        }
        for r in &self.reservations {
            out.push_str(&format!(
                "r {} {} {} {} {} {} {}\n",
                r.pid,
                r.start,
                r.instance,
                r.id,
                r.bytes,
                r.since_unix,
                sanitize(&r.role)
            ));
        }
        for p in &self.pins {
            let i = p.identity;
            out.push_str(&format!(
                "p {} {} {} {} {} {} {} {}\n",
                p.pid, p.start, p.instance, i.dev, i.ino, i.len, i.mtime_sec, i.mtime_nsec
            ));
        }
        out
    }

    /// Parse the file; a line that does not parse is dropped (a torn write cannot happen — the file is
    /// replaced by rename — but a hand edit can).
    fn decode(text: &str) -> Self {
        let mut state = Self::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<u64>().ok());
            let signed = |i: usize| f.get(i).and_then(|v| v.parse::<i64>().ok());
            match f.first() {
                Some(&"n") if f.len() >= 5 => {
                    if let (Some(pid), Some(start), Some(instance)) = (num(1), num(2), num(3)) {
                        state.members.push(PalwHostMemberV1 { pid: pid as u32, start, instance, kind: f[4].to_string() });
                    }
                }
                Some(&"r") if f.len() >= 8 => {
                    if let (Some(pid), Some(start), Some(instance), Some(id), Some(bytes), Some(since)) =
                        (num(1), num(2), num(3), num(4), num(5), num(6))
                    {
                        state.reservations.push(PalwHostReservationV1 {
                            pid: pid as u32,
                            start,
                            instance,
                            id,
                            bytes,
                            role: f[7].to_string(),
                            since_unix: since,
                        });
                    }
                }
                Some(&"p") if f.len() >= 9 => {
                    if let (Some(pid), Some(start), Some(instance), Some(dev), Some(ino), Some(len), Some(sec), Some(nsec)) =
                        (num(1), num(2), num(3), num(4), num(5), num(6), signed(7), signed(8))
                    {
                        state.pins.push(PalwHostPinV1 {
                            pid: pid as u32,
                            start,
                            instance,
                            identity: FileIdentityV1 { dev, ino, len, mtime_sec: sec, mtime_nsec: nsec },
                        });
                    }
                }
                _ => {}
            }
        }
        state
    }
}

fn sanitize(word: &str) -> String {
    let s: String = word.chars().map(|c| if c.is_whitespace() { '-' } else { c }).collect();
    if s.is_empty() { "-".to_string() } else { s }
}

/// Why the host refused a reservation: the arithmetic, and who holds the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwHostRefusalV1 {
    pub need_bytes: u64,
    pub host_bound_bytes: u64,
    pub host_reserved_bytes: u64,
    /// The node ledgers holding those reservations.
    pub holders: usize,
}

impl std::fmt::Display for PalwHostRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let gib = |b: u64| b as f64 / (1u64 << 30) as f64;
        write!(
            f,
            "the host ledger cannot cover {:.2} GiB: the host's bound is {:.2} GiB and {:.2} GiB of it is reserved by {} node(s) on \
             this host",
            gib(self.need_bytes),
            gib(self.host_bound_bytes),
            gib(self.host_reserved_bytes),
            self.holders
        )
    }
}

/// **An exclusive `flock(2)` on the lock file**, held until dropped — or until the process dies: the
/// kernel releases it with the descriptor, so a node killed mid-write leaves no lock behind. `libc`'s
/// call rather than std's `File::lock`, which is newer than the workspace's `rust-version` (1.88).
struct PalwHostFlockV1(File);

impl PalwHostFlockV1 {
    fn acquire(file: File) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            loop {
                // SAFETY: a descriptor this guard owns; LOCK_EX blocks until the lock is this process's.
                if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
                    return Ok(Self(file));
                }
                let e = std::io::Error::last_os_error();
                if e.kind() != std::io::ErrorKind::Interrupted {
                    return Err(e);
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = file;
            Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "the host ledger needs flock(2), which this platform has not"))
        }
    }
}

impl Drop for PalwHostFlockV1 {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: the descriptor is still open — the guard owns its file until after this call.
            unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}

/// **One process's handle on the host ledger.** `instance` tells two ledgers in one process apart (a
/// test runs several); a real node has one.
pub struct PalwHostLedgerV1 {
    dir: PathBuf,
    pid: u32,
    start: u64,
    instance: u64,
    /// Serialises this handle's own file work: `flock` is per open file description, and two threads of
    /// one process each opening the lock file would each get the lock.
    local: Mutex<()>,
    /// Whether an I/O failure was already said.
    warned: AtomicBool,
}

static INSTANCES: AtomicU64 = AtomicU64::new(1);

impl PalwHostLedgerV1 {
    /// **Open the host ledger in `dir`** — created if absent (mode 0700) — or say why not: a directory
    /// this process cannot create or write, or a platform without `flock(2)`, is a node without the host
    /// ledger, never a hold.
    pub fn open(dir: &Path) -> Result<Arc<Self>, String> {
        if cfg!(not(unix)) {
            return Err("the host ledger needs flock(2), which this platform has not".to_string());
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("{} cannot be created: {e}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
        let probe = dir.join(format!(".{PALW_HOST_LEDGER_FILE_V1}.probe.{}", std::process::id()));
        File::create(&probe).and_then(|mut f| f.write_all(b"probe")).map_err(|e| format!("{} is not writable: {e}", dir.display()))?;
        let _ = std::fs::remove_file(&probe);
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(PALW_HOST_LEDGER_LOCK_V1))
            .map_err(|e| format!("{} cannot hold the lock file: {e}", dir.display()))?;
        let pid = std::process::id();
        Ok(Arc::new(Self {
            dir: dir.to_path_buf(),
            pid,
            start: process_start_v1(pid).unwrap_or(0),
            instance: INSTANCES.fetch_add(1, Ordering::Relaxed),
            local: Mutex::new(()),
            warned: AtomicBool::new(false),
        }))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Run `f` on the file's live state under the exclusive lock; the state is written back when `f`
    /// says it changed it, or when a dead process's lines were pruned.
    fn with_state<T>(&self, f: impl FnOnce(&mut PalwHostLedgerStateV1) -> (T, bool)) -> std::io::Result<T> {
        let _local = self.local.lock().unwrap_or_else(|p| p.into_inner());
        let lock = PalwHostFlockV1::acquire(
            OpenOptions::new().create(true).truncate(false).read(true).write(true).open(self.dir.join(PALW_HOST_LEDGER_LOCK_V1))?,
        )?;
        let path = self.dir.join(PALW_HOST_LEDGER_FILE_V1);
        let mut text = String::new();
        match File::open(&path) {
            Ok(mut file) => {
                file.read_to_string(&mut text)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut state = PalwHostLedgerStateV1::decode(&text);
        // A line of a process that is gone (or of a reused pid) is nobody's.
        let pruned = state.prune_dead();
        let (out, changed) = f(&mut state);
        if changed || pruned {
            let tmp = self.dir.join(format!(".{PALW_HOST_LEDGER_FILE_V1}.{}.{}", self.pid, self.instance));
            File::create(&tmp)?.write_all(state.encode().as_bytes())?;
            std::fs::rename(&tmp, &path)?;
        }
        drop(lock);
        Ok(out)
    }

    /// Say an I/O failure once; the node's own bound decides meanwhile.
    fn note_failure(&self, what: &str, e: &std::io::Error) {
        if !self.warned.swap(true, Ordering::Relaxed) {
            warn!(
                "[palw-host] the host ledger in {} failed to {what} ({e}): this node's own memory ledger decides alone until it \
                 recovers — the int-10.1 behaviour (int-10.2 D1)",
                self.dir.display()
            );
        }
    }

    /// **Say who this process is** (an `n` line; once per handle): `host_nodes` and `host_pinner` count them.
    pub fn register_member(&self, kind: &str) {
        let (pid, start, instance) = (self.pid, self.start, self.instance);
        if let Err(e) = self.with_state(|state| {
            if state.members.iter().any(|m| m.pid == pid && m.instance == instance) {
                return ((), false);
            }
            state.members.push(PalwHostMemberV1 { pid, start, instance, kind: kind.to_string() });
            ((), true)
        }) {
            self.note_failure("register this process", &e);
        }
    }

    /// **Reserve `bytes` on the host for this node's reservation `id`**, against `host_bound` (`None`: the
    /// platform cannot say, and the host term binds nothing — it still registers, so the others see it).
    /// `Err` is the host's refusal, with its numbers. An I/O failure grants: see the module doc.
    pub fn try_reserve(&self, id: u64, bytes: u64, role: &str, host_bound: Option<u64>) -> Result<(), PalwHostRefusalV1> {
        let (pid, start, instance) = (self.pid, self.start, self.instance);
        let outcome = self.with_state(|state| {
            if let Some(bound) = host_bound {
                let reserved = state.reserved_bytes();
                if bytes > bound.saturating_sub(reserved) {
                    let refusal = PalwHostRefusalV1 {
                        need_bytes: bytes,
                        host_bound_bytes: bound,
                        host_reserved_bytes: reserved,
                        holders: state.reserving(),
                    };
                    return (Err(refusal), false);
                }
            }
            state.reservations.push(PalwHostReservationV1 {
                pid,
                start,
                instance,
                id,
                bytes,
                role: role.to_string(),
                since_unix: unix_now_secs(),
            });
            (Ok(()), true)
        });
        match outcome {
            Ok(result) => result,
            Err(e) => {
                self.note_failure("register a reservation", &e);
                Ok(())
            }
        }
    }

    /// **Whether `bytes` could be reserved on the host now** — the dry run; registers nothing.
    pub fn can_reserve(&self, bytes: u64, host_bound: Option<u64>) -> Result<(), PalwHostRefusalV1> {
        let Some(bound) = host_bound else { return Ok(()) };
        match self.with_state(|state| {
            let reserved = state.reserved_bytes();
            let refusal = (bytes > bound.saturating_sub(reserved)).then(|| PalwHostRefusalV1 {
                need_bytes: bytes,
                host_bound_bytes: bound,
                host_reserved_bytes: reserved,
                holders: state.reserving(),
            });
            (refusal, false)
        }) {
            Ok(Some(refusal)) => Err(refusal),
            Ok(None) => Ok(()),
            Err(e) => {
                self.note_failure("read the host's reservations", &e);
                Ok(())
            }
        }
    }

    /// Release this node's reservation `id`.
    pub fn release(&self, id: u64) {
        let (pid, instance) = (self.pid, self.instance);
        if let Err(e) = self.with_state(|state| {
            let before = state.reservations.len();
            state.reservations.retain(|r| !(r.pid == pid && r.instance == instance && r.id == id));
            ((), state.reservations.len() != before)
        }) {
            self.note_failure("release a reservation", &e);
        }
    }

    /// **Register a file this process pinned** (once per identity per process).
    pub fn register_pin(&self, identity: FileIdentityV1) {
        let (pid, start, instance) = (self.pid, self.start, self.instance);
        if let Err(e) = self.with_state(|state| {
            if state.pins.iter().any(|p| p.pid == pid && p.instance == instance && p.identity == identity) {
                return ((), false);
            }
            state.pins.push(PalwHostPinV1 { pid, start, instance, identity });
            ((), true)
        }) {
            self.note_failure("register a pin", &e);
        }
    }

    /// Unregister a file this process unpinned.
    pub fn unregister_pin(&self, identity: FileIdentityV1) {
        let (pid, instance) = (self.pid, self.instance);
        if let Err(e) = self.with_state(|state| {
            let before = state.pins.len();
            state.pins.retain(|p| !(p.pid == pid && p.instance == instance && p.identity == identity));
            ((), state.pins.len() != before)
        }) {
            self.note_failure("unregister a pin", &e);
        }
    }

    /// The host's live state, as the file says it now.
    pub fn snapshot(&self) -> Option<PalwHostLedgerStateV1> {
        self.with_state(|state| (state.clone(), false)).ok()
    }

    /// Drop every line this handle wrote — a process going away cleanly (the next writer would prune a
    /// dead process's lines anyway).
    pub fn forget_mine(&self) {
        let (pid, instance) = (self.pid, self.instance);
        let _ = self.with_state(|state| {
            let before = (state.members.len(), state.reservations.len(), state.pins.len());
            state.members.retain(|m| !(m.pid == pid && m.instance == instance));
            state.reservations.retain(|r| !(r.pid == pid && r.instance == instance));
            state.pins.retain(|p| !(p.pid == pid && p.instance == instance));
            ((), before != (state.members.len(), state.reservations.len(), state.pins.len()))
        });
    }
}

fn unix_now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// **A process's start time**, where the platform keeps it cheaply: Linux's `/proc/<pid>/stat` field 22
/// (clock ticks since boot). `None` elsewhere — liveness is then the pid's alone.
pub fn process_start_v1(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The command name is in parentheses and may contain spaces: count from after it (field 3 on).
        let after = &text[text.rfind(')')? + 1..];
        after.split_whitespace().nth(19)?.parse().ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// **Whether the process a line names is the one alive now**: the pid exists (`kill(pid, 0)` is not
/// ESRCH) and, where the platform keeps a start time, it is the one recorded (a reused pid is another
/// process).
pub fn process_is_v1(pid: u32, start: u64) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        // SAFETY: signal 0 checks existence and permission and sends nothing.
        let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if rc != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            return false;
        }
    }
    match (start, process_start_v1(pid)) {
        (0, _) | (_, None) => true,
        (recorded, Some(now)) => recorded == now,
    }
}

// -------------------------------------------------------------------------------------------------
// The process's handle
// -------------------------------------------------------------------------------------------------

static SHARED: OnceLock<Option<Arc<PalwHostLedgerV1>>> = OnceLock::new();

/// **Arm the shared host ledger** from `--palw-host-ledger-dir`, once — from the daemon (`kind` =
/// [`PALW_HOST_MEMBER_NODE_V1`]) or the host pinner ([`PALW_HOST_MEMBER_PINNER_V1`]) — and register this
/// process in it. `None` (the default) leaves it off; a directory that cannot be used is logged and
/// leaves it off. Returns the handle.
pub fn arm_shared_host_ledger_v1(dir: Option<&Path>, kind: &'static str) -> Option<Arc<PalwHostLedgerV1>> {
    SHARED
        .get_or_init(|| {
            let dir = dir?;
            match PalwHostLedgerV1::open(dir) {
                Ok(host) => {
                    host.register_member(kind);
                    Some(host)
                }
                Err(why) => {
                    warn!(
                        "[palw-host] --palw-host-ledger-dir: {why} — this process runs without the host ledger: a node's memory \
                         ledger bounds its own duties only, as before int-10.2"
                    );
                    None
                }
            }
        })
        .clone()
}

/// The process's shared host ledger, if one is armed.
pub fn shared_host_ledger_v1() -> Option<Arc<PalwHostLedgerV1>> {
    SHARED.get().cloned().flatten()
}

/// **The host half of `getPalwNodeStatus.verification`**: `host_ledger=off`, or — as the file says it
/// now — the host's nodes and whether a pinner runs, the bytes reserved on it and by how many nodes, and
/// its distinct pinned files (each counted once). New keys in the existing `key=value` line.
pub fn host_ledger_status_v1() -> String {
    match shared_host_ledger_v1().and_then(|host| host.snapshot()) {
        None => "host_ledger=off".to_string(),
        Some(state) => {
            let (files, pinned) = state.distinct_pinned();
            format!(
                "host_ledger=on host_nodes={} host_pinner={} host_reserved_mib={} host_reserving={} host_pinned_mib={} \
                 host_pinned_files={files}",
                state.members_of(PALW_HOST_MEMBER_NODE_V1),
                state.members_of(PALW_HOST_MEMBER_PINNER_V1),
                state.reserved_bytes() >> 20,
                state.reserving(),
                pinned >> 20
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(n: u64, len: u64) -> FileIdentityV1 {
        FileIdentityV1 { dev: 7, ino: n, len, mtime_sec: 1_759_300_000, mtime_nsec: 0 }
    }

    /// **The file round-trips, and what it counts is what a host is.** Members are counted by kind;
    /// reservations sum, by the ledgers that hold them; two processes pinning one file count it once; a
    /// line that does not parse is dropped.
    #[test]
    fn the_host_ledger_file_round_trips_and_counts_a_shared_pin_once() {
        let state = PalwHostLedgerStateV1 {
            members: vec![
                PalwHostMemberV1 { pid: 9, start: 0, instance: 1, kind: PALW_HOST_MEMBER_PINNER_V1.into() },
                PalwHostMemberV1 { pid: 10, start: 1, instance: 1, kind: PALW_HOST_MEMBER_NODE_V1.into() },
                PalwHostMemberV1 { pid: 11, start: 2, instance: 1, kind: PALW_HOST_MEMBER_NODE_V1.into() },
            ],
            reservations: vec![
                PalwHostReservationV1 {
                    pid: 10,
                    start: 1,
                    instance: 1,
                    id: 1,
                    bytes: 1 << 30,
                    role: "full-seat".into(),
                    since_unix: 5,
                },
                PalwHostReservationV1 { pid: 11, start: 2, instance: 1, id: 9, bytes: 1 << 29, role: "a role".into(), since_unix: 6 },
                PalwHostReservationV1 { pid: 11, start: 2, instance: 1, id: 10, bytes: 1 << 29, role: "court".into(), since_unix: 7 },
            ],
            pins: vec![
                PalwHostPinV1 { pid: 9, start: 0, instance: 1, identity: identity(42, 1_799_359_436) },
                PalwHostPinV1 { pid: 10, start: 1, instance: 1, identity: identity(42, 1_799_359_436) },
                PalwHostPinV1 { pid: 11, start: 2, instance: 1, identity: identity(43, 1_866_691_136) },
            ],
        };
        let text = state.encode() + "garbage line\nr 1 2\nn 5\n";
        let back = PalwHostLedgerStateV1::decode(&text);
        assert_eq!(back.members, state.members);
        assert_eq!(back.reservations.len(), 3);
        assert_eq!(back.reservations[1].role, "a-role", "a role is one token");
        assert_eq!(back.pins, state.pins);
        assert_eq!((back.members_of(PALW_HOST_MEMBER_NODE_V1), back.members_of(PALW_HOST_MEMBER_PINNER_V1)), (2, 1));
        assert_eq!(back.reserved_bytes(), (1 << 30) + (1 << 29) + (1 << 29));
        assert_eq!(back.reserving(), 2, "two ledgers hold the three reservations");
        assert_eq!(back.distinct_pinned(), (2, 1_799_359_436 + 1_866_691_136), "the 8k file once, the IR file once");
    }
}
