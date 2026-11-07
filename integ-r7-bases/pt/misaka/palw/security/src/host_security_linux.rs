//! **The Linux confinement backend — `seccomp` + `Landlock`** (ADR-0079 Decision 5's platform
//! half, unit R-04). Declared from [`super`] with `#[path]`; it exists on no other platform.
//!
//! The rule this file obeys is the one the macOS backend established: **a backend that did not
//! install must never reach the report as the value someone configured.** The only constructor
//! that yields a non-`none` [`Confinement`] is [`super::establish_confinement`], and it runs
//! [`establish`] — which returns `none` with a reason unless its drill has *observed* every
//! denial it promises (S3, S12).
//!
//! **What this backend delivers, stated exactly** — a partial control described as a full one is
//! worse than none:
//!
//! * **no network**: `socket`, `connect`, `bind` and their neighbours return `EPERM` from a
//!   `seccomp` filter, so there is no socket to reach a network with;
//! * **no writes** outside the working directory, the outbox and `/dev/null`;
//! * **no reads** outside the system start-up set, the artifact paths named by the allowlisted
//!   environment, and the writable set. This is the half macOS could not deliver, and the drill
//!   proves it with a differential probe: a file readable UNCONFINED is refused under the profile.
//! * **no `ptrace`, no `process_vm_*`, no mount/namespace calls, no module or key syscalls.**
//!
//! **What it does not deliver:**
//!
//! * **`execve` is allowed by the pre-exec filter, necessarily.** The filter is installed between
//!   `fork` and `execve` (that is the only place a supervisor can confine a child it does not
//!   own), so a filter denying `execve` would deny the worker's own start. Decision 5's "no
//!   `execve` after setup" is therefore a SECOND filter, stacked by the worker itself:
//!   [`confine_self_after_exec`] at the top of `main`. Seccomp filters stack, and the stacked one
//!   cannot be removed. A worker binary that does not call it keeps the first filter and loses
//!   only the `execve` denial — which is why the function reports whether it stacked anything.
//! * **no protection against a dishonest executor.** Nothing here is visible to a peer and nothing
//!   here can change a root (ADR-0079 Decision 3). A host that deviates is convicted by the court,
//!   by arithmetic, with its bond — confinement adds nothing to that and is never asked to.
//!
//! **Why raw syscalls and not the `landlock` / `seccompiler` crates.** The install runs in the
//! child between `fork()` and `execve()`. Between those two points a process may not allocate: if
//! another thread held the allocator's lock at `fork`, the child's `malloc` deadlocks forever, and
//! a supervisor that hangs one worker per job is a worse failure than the one this file prevents.
//! So everything that allocates — opening the rule fds, building the BPF program — happens in the
//! PARENT, and the child performs only bare syscalls on values it was handed. The `landlock`
//! crate's `restrict_self` consumes its ruleset by value, which a `pre_exec` closure (`FnMut`)
//! cannot do, and its rule building allocates; adapting it would leave nothing of it but the
//! syscall numbers this file already names. The kernel's ABI is the dependency either way.

use std::ffi::{CString, c_char};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use super::{Confinement, ConfinementBackend, ExecveDenial, declare_backend_in_force};

// -------------------------------------------------------------------------------------------
// The kernel ABI, named here rather than depended on
// -------------------------------------------------------------------------------------------

/// Landlock's three syscalls. The numbers are the same on every architecture: they were allocated
/// after the kernel unified new-syscall numbering, so there is no per-arch table to get wrong.
const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
const SYS_LANDLOCK_ADD_RULE: libc::c_long = 445;
const SYS_LANDLOCK_RESTRICT_SELF: libc::c_long = 446;

/// `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)` answers with the ABI level.
/// **This is the probe, and it is the only honest one**: a kernel version string says what the
/// source tree contained, not what this kernel booted with.
const LANDLOCK_CREATE_RULESET_VERSION: libc::c_ulong = 1;
const LANDLOCK_RULE_PATH_BENEATH: libc::c_ulong = 1;

const FS_EXECUTE: u64 = 1 << 0;
const FS_WRITE_FILE: u64 = 1 << 1;
const FS_READ_FILE: u64 = 1 << 2;
const FS_READ_DIR: u64 = 1 << 3;
const FS_REMOVE_DIR: u64 = 1 << 4;
const FS_REMOVE_FILE: u64 = 1 << 5;
const FS_MAKE_CHAR: u64 = 1 << 6;
const FS_MAKE_DIR: u64 = 1 << 7;
const FS_MAKE_REG: u64 = 1 << 8;
const FS_MAKE_SOCK: u64 = 1 << 9;
const FS_MAKE_FIFO: u64 = 1 << 10;
const FS_MAKE_BLOCK: u64 = 1 << 11;
const FS_MAKE_SYM: u64 = 1 << 12;
/// ABI 2.
const FS_REFER: u64 = 1 << 13;
/// ABI 3.
const FS_TRUNCATE: u64 = 1 << 14;

/// Every access an ABI-1 kernel handles.
const FS_ABI1: u64 = FS_EXECUTE
    | FS_WRITE_FILE
    | FS_READ_FILE
    | FS_READ_DIR
    | FS_REMOVE_DIR
    | FS_REMOVE_FILE
    | FS_MAKE_CHAR
    | FS_MAKE_DIR
    | FS_MAKE_REG
    | FS_MAKE_SOCK
    | FS_MAKE_FIFO
    | FS_MAKE_BLOCK
    | FS_MAKE_SYM;

/// The accesses the kernel will accept on a rule whose target is **not** a directory. A rule that
/// names a file and asks for a directory-only right is `EINVAL`, so the plan masks by inode kind.
const FS_FILE_APPLICABLE: u64 = FS_EXECUTE | FS_WRITE_FILE | FS_READ_FILE | FS_TRUNCATE;

/// What the ruleset declares it handles. **`LANDLOCK_ACCESS_FS_IOCTL_DEV` (ABI 5) is deliberately
/// absent**: handling it would deny every `ioctl` on a character device the ruleset does not grant
/// it on — `isatty` on an inherited terminal included — and this backend restricts what the
/// arithmetic forbids, not what the kernel happens to offer.
fn handled_access_fs(abi: i32) -> u64 {
    let mut handled = FS_ABI1;
    if abi >= 2 {
        handled |= FS_REFER;
    }
    if abi >= 3 {
        handled |= FS_TRUNCATE;
    }
    handled
}

#[repr(C)]
struct RulesetAttrV1 {
    handled_access_fs: u64,
}

#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

/// `prctl` numbers, spelled here so a `libc` that renames one cannot silently change the meaning.
const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
const PR_GET_SECCOMP: libc::c_int = 21;

const SECCOMP_SET_MODE_FILTER: libc::c_ulong = 1;
const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;

const BPF_LD_W_ABS: u16 = 0x20;
const BPF_JMP_JEQ_K: u16 = 0x15;
const BPF_JMP_JGE_K: u16 = 0x35;
const BPF_RET_K: u16 = 0x06;

/// `offsetof(struct seccomp_data, nr)` and `…, arch)`.
const SECCOMP_DATA_NR: u32 = 0;
const SECCOMP_DATA_ARCH: u32 = 4;

#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH: u32 = 0xc000_003e;
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH: u32 = 0xc000_00b7;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const AUDIT_ARCH: u32 = 0;

/// The filter is written for the two architectures this lineage runs on. Anywhere else the
/// backend reports `none` rather than installing a filter whose `AUDIT_ARCH` is a guess.
const ARCH_SUPPORTED: bool = cfg!(any(target_arch = "x86_64", target_arch = "aarch64"));

// -------------------------------------------------------------------------------------------
// The capability list, READ OFF the determinism rules (ADR-0079 Decision 1)
// -------------------------------------------------------------------------------------------

/// The syscalls the pre-exec filter refuses. Each line is a capability the arithmetic does not
/// need, and whose absence is a property the court already relies on:
///
/// * the socket family — *a network read is a thing two hosts disagree about*;
/// * `ptrace` / `process_vm_*` — a job process has no business in another process's memory, and
///   this is the pair that would reach the signer sidecar;
/// * mount / namespace calls — a job process does not reshape the host's filesystem view, and
///   `unshare(CLONE_NEWUSER)` is the standard way out of a filesystem confinement;
/// * module, key and `bpf` syscalls — kernel-surface calls with no place in an inference.
///
/// `execve` is NOT here; see the module docs and [`confine_self_after_exec`].
fn denied_syscalls() -> Vec<libc::c_long> {
    vec![
        libc::SYS_socket,
        libc::SYS_socketpair,
        libc::SYS_connect,
        libc::SYS_accept,
        libc::SYS_accept4,
        libc::SYS_bind,
        libc::SYS_listen,
        libc::SYS_sendto,
        libc::SYS_recvfrom,
        libc::SYS_sendmsg,
        libc::SYS_recvmsg,
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_chroot,
        libc::SYS_setns,
        libc::SYS_unshare,
        libc::SYS_init_module,
        libc::SYS_finit_module,
        libc::SYS_delete_module,
        libc::SYS_kexec_load,
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_add_key,
        libc::SYS_request_key,
        libc::SYS_keyctl,
    ]
}

/// Decision 5's "no `execve` after setup", as the filter the worker stacks on itself.
fn denied_after_exec() -> Vec<libc::c_long> {
    vec![libc::SYS_execve, libc::SYS_execveat]
}

/// **The system read set, MEASURED — not assumed.** A dynamically linked binary cannot reach its
/// own `main` without the loader and the shared objects it names; a read set narrowed below this
/// aborts the child before `main`, which is the failure that left the macOS backend with a broad
/// read set.
///
/// Measured on the fleet's own host (Ubuntu 6.8.0-124, x86_64, merged `/usr`, Landlock ABI 4) by
/// ablation — the drill's exec leg, and separately a real dynamically linked **Rust** binary, run
/// with each entry removed in turn:
///
/// | read set | a dynamically linked child |
/// |---|---|
/// | `/usr` + `/lib` `/lib64` `/bin` `/sbin` + `/etc/ld.so.cache` | starts |
/// | the same without `/etc/ld.so.cache` | starts |
/// | `/usr` alone | starts |
/// | `/usr` alone, no `/etc/ld.so.cache` | starts (a real Rust binary too) |
/// | `/lib` `/lib64` `/bin` `/sbin` alone (symlinks into `/usr`) | starts |
/// | nothing | **cannot start** — `execve` fails before `main` |
///
/// So the measured minimum is `/usr`. `/lib`, `/lib64`, `/bin` and `/sbin` are symlinks into
/// `/usr` on a merged-`/usr` distribution — a rule on them resolves to a rule already held, so
/// they grant nothing extra here — and are real directories on a split-`/usr` host and every musl
/// container, where they are the whole loader path. They are skipped where absent.
///
/// **`/etc/ld.so.cache` is deliberately NOT granted**: the ablation shows the loader starts
/// without it, falling back to its default search path. The residual, stated: a worker linked
/// against a library that only the cache can locate — an `ldconfig`'d directory outside the
/// loader's default search path, `/usr/local/lib` being the usual one — will not find it, and the
/// supervisor's boot identity probe fails loudly on a backend that is opt-in. Granting one more
/// read is then a one-line reviewed change, which is the direction this ADR wants the friction in.
///
/// **What else is NOT here, and why:** `/proc` (`current_exe()` is a `readlink`, which Landlock
/// does not gate; the worker then opens its own binary, which [`confined_command`] grants by
/// name), `/dev/urandom` (the determinism rules forbid randomness, and `std`'s hasher seed comes
/// from the `getrandom` syscall rather than a device), `/etc`, and the operator's home.
const SYSTEM_READ_EXEC: &[&str] = &["/usr", "/lib", "/lib64", "/bin", "/sbin"];

/// Read-only FILES outside the directories above. Empty by measurement, not by omission: see the
/// ablation table on [`SYSTEM_READ_EXEC`]. A name added here is one more thing a hostile GGUF
/// parser can read, so each one needs the drill to say it is necessary.
const SYSTEM_READ_FILE: &[&str] = &[];

/// `/dev/null` is the one device a confined job may touch: the supervisor hands children a null
/// stdin, and code that opens it by name is code that would otherwise fail for no security reason.
const DEVICE_READ_WRITE: &[&str] = &["/dev/null"];

/// Programs the drill may exec to prove that a dynamically linked child starts under the profile.
const EXEC_PROBE_CANDIDATES: &[&str] = &["/bin/true", "/usr/bin/true", "/bin/echo", "/usr/bin/echo"];

/// Files the drill may try to read to prove that a read OUTSIDE the set is refused. The probe is
/// differential: whichever is chosen must be readable UNCONFINED first, or the leg is inconclusive
/// and the backend reports `none`.
const READ_OUTSIDE_CANDIDATES: &[&str] = &["/etc/hostname", "/etc/hosts", "/etc/os-release"];

// -------------------------------------------------------------------------------------------
// The plan: everything that allocates, built in the parent
// -------------------------------------------------------------------------------------------

/// One Landlock rule: a path and the accesses granted beneath it.
#[derive(Clone)]
struct Rule {
    path: PathBuf,
    access: u64,
}

/// The confinement, resolved against this host and ready to install. Everything expensive is here
/// so that the child between `fork` and `execve` does nothing but syscalls.
pub(super) struct Plan {
    abi: i32,
    handled: u64,
    rules: Vec<Rule>,
    bpf: Arc<Vec<libc::sock_filter>>,
    read_set: Vec<PathBuf>,
    write_set: Vec<PathBuf>,
}

impl std::fmt::Debug for Plan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plan")
            .field("landlock_abi", &self.abi)
            .field("rules", &self.rules.len())
            .field("denied_syscalls", &(self.bpf.len()))
            .finish()
    }
}

fn is_dir(path: &Path) -> bool {
    std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

/// The accesses a writable directory needs: the outbox is written, re-written, and swept.
fn write_dir_access(handled: u64) -> u64 {
    (FS_READ_FILE | FS_READ_DIR | FS_WRITE_FILE | FS_MAKE_REG | FS_MAKE_DIR | FS_REMOVE_FILE | FS_REMOVE_DIR | FS_TRUNCATE) & handled
}

impl Plan {
    /// Resolve the plan against this host. Paths that do not exist are skipped — a class whose
    /// worker reads no artifact variable is a class with no artifact path, not an error.
    fn build(abi: i32, workdir: &Path, writable: &[PathBuf]) -> Self {
        let handled = handled_access_fs(abi);
        let mut rules: Vec<Rule> = Vec::new();
        let mut read_set: Vec<PathBuf> = Vec::new();
        let mut write_set: Vec<PathBuf> = Vec::new();

        let push = |rules: &mut Vec<Rule>, path: PathBuf, access: u64| {
            if !path.exists() {
                return false;
            }
            let access = if is_dir(&path) { access & handled } else { access & FS_FILE_APPLICABLE & handled };
            if access == 0 || rules.iter().any(|r| r.path == path) {
                return false;
            }
            rules.push(Rule { path, access });
            true
        };

        for entry in SYSTEM_READ_EXEC {
            let path = PathBuf::from(entry);
            if push(&mut rules, path.clone(), FS_READ_FILE | FS_READ_DIR | FS_EXECUTE) {
                read_set.push(path);
            }
        }
        for entry in SYSTEM_READ_FILE {
            let path = PathBuf::from(entry);
            if push(&mut rules, path.clone(), FS_READ_FILE) {
                read_set.push(path);
            }
        }
        for entry in DEVICE_READ_WRITE {
            let path = PathBuf::from(entry);
            if push(&mut rules, path.clone(), FS_READ_FILE | FS_WRITE_FILE) {
                read_set.push(path);
            }
        }
        // The artifact and golden paths: read-only, and derived from the SAME constant that says
        // what the child's environment is, so a name added to the allowlist cannot become a path
        // the child can name but not open.
        for path in artifact_read_paths() {
            if push(&mut rules, path.clone(), FS_READ_FILE | FS_READ_DIR | FS_EXECUTE) {
                read_set.push(path);
            }
        }
        // The outbox and the working directory: the only places anything may be written.
        let mut writable_all: Vec<PathBuf> = vec![workdir.to_path_buf()];
        writable_all.extend(writable.iter().cloned());
        for path in writable_all {
            if push(&mut rules, path.clone(), write_dir_access(handled)) {
                write_set.push(path);
            }
        }

        Plan { abi, handled, rules, bpf: Arc::new(deny_filter(&denied_syscalls())), read_set, write_set }
    }

    /// Create a ruleset fd carrying this plan's rules, plus (for a spawn) the program itself:
    /// `execve` needs `LANDLOCK_ACCESS_FS_EXECUTE` on the binary, and the worker lives wherever
    /// the operator built it, which is nowhere in the system read set.
    fn ruleset(&self, program: Option<&Path>) -> Result<OwnedFd, String> {
        let attr = RulesetAttrV1 { handled_access_fs: self.handled };
        let fd = unsafe {
            libc::syscall(SYS_LANDLOCK_CREATE_RULESET, &attr as *const RulesetAttrV1, std::mem::size_of::<RulesetAttrV1>(), 0u64)
        };
        if fd < 0 {
            return Err(format!("landlock_create_ruleset failed: {}", std::io::Error::last_os_error()));
        }
        let ruleset = unsafe { OwnedFd::from_raw_fd(fd as RawFd) };
        for rule in &self.rules {
            add_rule(&ruleset, &rule.path, rule.access)?;
        }
        if let Some(program) = program {
            // Read as well as execute: the worker hashes its own binary at boot
            // (`worker_binary_sha256`), and a self-hash that cannot read itself is a dead worker.
            add_rule(&ruleset, program, (FS_EXECUTE | FS_READ_FILE) & self.handled)?;
        }
        Ok(ruleset)
    }

    fn notes(&self) -> Vec<String> {
        let list = |paths: &[PathBuf]| paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" ");
        vec![
            format!("landlock: ABI {}, {} rules, handled access mask {:#x}", self.abi, self.rules.len(), self.handled),
            format!("landlock read set (read-only; /dev/null is the one writable device): {}", list(&self.read_set)),
            format!("landlock writable set: {}", list(&self.write_set)),
            format!(
                "seccomp: {} syscalls denied with EPERM (socket family, ptrace, process_vm_*, mount/namespace, module/key)",
                denied_syscalls().len()
            ),
        ]
    }
}

/// The artifact paths the child may read, taken from the delivered environment. Only the
/// `MISAKA_PALW_*` names carry paths; the pinned locale values do not.
fn artifact_read_paths() -> Vec<PathBuf> {
    let delivered = super::worker_environment();
    let mut out = Vec::new();
    for (name, value) in &delivered.vars {
        if !name.starts_with("MISAKA_PALW_") {
            continue;
        }
        let path = PathBuf::from(value);
        if path.is_absolute() && path.exists() {
            out.push(path);
        }
    }
    out
}

fn add_rule(ruleset: &OwnedFd, path: &Path, access: u64) -> Result<(), String> {
    if access == 0 {
        return Ok(());
    }
    let c_path = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| format!("{} has a NUL in it", path.display()))?;
    // O_PATH is enough to name an inode for a rule and opens nothing: it needs no read permission
    // and triggers no side effect on a device or a FIFO.
    let parent_fd = unsafe { libc::open(c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if parent_fd < 0 {
        return Err(format!("cannot open {} to name it in a rule: {}", path.display(), std::io::Error::last_os_error()));
    }
    let owned = unsafe { OwnedFd::from_raw_fd(parent_fd) };
    let attr = PathBeneathAttr { allowed_access: access, parent_fd: owned.as_raw_fd() };
    let rc = unsafe {
        libc::syscall(SYS_LANDLOCK_ADD_RULE, ruleset.as_raw_fd(), LANDLOCK_RULE_PATH_BENEATH, &attr as *const PathBeneathAttr, 0u64)
    };
    if rc != 0 {
        return Err(format!("landlock_add_rule({}) failed: {}", path.display(), std::io::Error::last_os_error()));
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// The BPF program
// -------------------------------------------------------------------------------------------

fn stmt(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter { code, jt: 0, jf: 0, k }
}

fn jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

/// A classic-BPF program that returns `EPERM` for the named syscalls and allows everything else.
///
/// `EPERM` rather than `SECCOMP_RET_KILL`: ADR-0079 Decision 3 says a denied syscall is a
/// `JobFailed` **with the denial named**, never a different number — and a killed process names
/// nothing. A wrong-architecture call IS killed, because a filter that does not know which table
/// the numbers came from is not a filter.
fn deny_filter(denied: &[libc::c_long]) -> Vec<libc::sock_filter> {
    assert!(denied.len() < 250, "a longer deny list needs a jump table, not a linear scan");
    let n = denied.len() as u8;
    let mut prog = Vec::with_capacity(denied.len() + 6);
    prog.push(stmt(BPF_LD_W_ABS, SECCOMP_DATA_ARCH));
    prog.push(jump(BPF_JMP_JEQ_K, AUDIT_ARCH, 1, 0));
    prog.push(stmt(BPF_RET_K, SECCOMP_RET_KILL_PROCESS));
    prog.push(stmt(BPF_LD_W_ABS, SECCOMP_DATA_NR));
    if cfg!(target_arch = "x86_64") {
        // The x32 ABI reaches the same table with bit 30 set; a filter that ignored it would let
        // every denied call through under a different number.
        prog.push(jump(BPF_JMP_JGE_K, 0x4000_0000, 0, 1));
        prog.push(stmt(BPF_RET_K, SECCOMP_RET_KILL_PROCESS));
    }
    for (i, nr) in denied.iter().enumerate() {
        // Jump to the EPERM return: the remaining comparisons, then the ALLOW return.
        prog.push(jump(BPF_JMP_JEQ_K, *nr as u32, n - i as u8, 0));
    }
    prog.push(stmt(BPF_RET_K, SECCOMP_RET_ALLOW));
    prog.push(stmt(BPF_RET_K, SECCOMP_RET_ERRNO | (libc::EPERM as u32 & 0xffff)));
    prog
}

// -------------------------------------------------------------------------------------------
// The install — the only code that runs between fork() and execve()
// -------------------------------------------------------------------------------------------

/// Install the confinement in the CURRENT process. **Allocation-free by construction**: it takes
/// an already-open ruleset fd and an already-built program, and makes four syscalls.
///
/// # Safety
/// Must be called on a process (or a just-forked child) that is about to be confined for good:
/// none of this can be undone.
unsafe fn install(ruleset: Option<RawFd>, bpf: Option<&[libc::sock_filter]>) -> Result<(), std::io::Error> {
    if unsafe { libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if let Some(fd) = ruleset
        && unsafe { libc::syscall(SYS_LANDLOCK_RESTRICT_SELF, fd, 0u64) } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    if let Some(prog) = bpf
        && unsafe { apply_seccomp(prog) } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// # Safety
/// Installs a seccomp filter on the calling thread; irreversible.
unsafe fn apply_seccomp(prog: &[libc::sock_filter]) -> libc::c_long {
    let fprog = libc::sock_fprog { len: prog.len() as libc::c_ushort, filter: prog.as_ptr() as *mut libc::sock_filter };
    unsafe { libc::syscall(libc::SYS_seccomp, SECCOMP_SET_MODE_FILTER, 0u64, &fprog as *const libc::sock_fprog) }
}

// -------------------------------------------------------------------------------------------
// Decision 5's second half: the worker denies its own execve
// -------------------------------------------------------------------------------------------

/// `SECCOMP_MODE_FILTER`, the answer `prctl(PR_GET_SECCOMP)` gives a process that is already
/// filtered — which is to say, a process a confining supervisor spawned.
const SECCOMP_MODE_FILTER: libc::c_int = 2;

pub(super) fn confine_self_after_exec() -> ExecveDenial {
    if !ARCH_SUPPORTED {
        return ExecveDenial::Unsupported("the seccomp filter in this build is written for x86_64 and aarch64".into());
    }
    // `PR_GET_SECCOMP` is the question "am I already filtered?", asked of the kernel. It is safe
    // to ask under a filter because this backend's filter allows `prctl` — and a filter that
    // denied it would kill us here rather than answer, which is a failure mode this cannot hide.
    let mode = unsafe { libc::prctl(PR_GET_SECCOMP) };
    if mode < 0 {
        return ExecveDenial::Unsupported(format!("prctl(PR_GET_SECCOMP): {}", std::io::Error::last_os_error()));
    }
    if mode != SECCOMP_MODE_FILTER {
        return ExecveDenial::NotConfined;
    }
    let prog = deny_filter(&denied_after_exec());
    match unsafe { stack_execve_denial(&prog) } {
        Ok(()) => ExecveDenial::Stacked,
        Err(e) => ExecveDenial::Failed(e.to_string()),
    }
}

/// The allocation-free half of [`confine_self_after_exec`], so the drill can exercise exactly
/// this code in a forked child without allocating there.
///
/// # Safety
/// Installs an irreversible seccomp filter on the calling thread.
unsafe fn stack_execve_denial(prog: &[libc::sock_filter]) -> Result<(), std::io::Error> {
    if unsafe { libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { apply_seccomp(prog) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// The spawn
// -------------------------------------------------------------------------------------------

/// Build a command whose child confines itself in `pre_exec`.
///
/// If the ruleset cannot be built the command is returned with a `pre_exec` that FAILS: the spawn
/// errors instead of quietly running unconfined, because by this point the report already says
/// this host is confined and a silent downgrade would make that report a lie.
pub(super) fn confined_command(plan: &Arc<Plan>, program: &Path) -> Command {
    let mut cmd = Command::new(program);
    match plan.ruleset(Some(program)) {
        Ok(fd) => {
            let fd = Arc::new(fd);
            let bpf = Arc::clone(&plan.bpf);
            // Between fork() and execve(): syscalls only, no allocation, no locks.
            let confine = move || unsafe { install(Some(fd.as_raw_fd()), Some(bpf.as_slice())) };
            // SAFETY: `confine` makes only async-signal-safe syscalls on values built above.
            unsafe { cmd.pre_exec(confine) };
        }
        Err(why) => {
            eprintln!("[palw-confinement] cannot build the Landlock ruleset for {}: {why} — the spawn will fail", program.display());
            // SAFETY: the closure allocates nothing and only builds an error value.
            unsafe { cmd.pre_exec(move || Err(std::io::Error::from_raw_os_error(libc::EPERM))) };
        }
    }
    cmd
}

// -------------------------------------------------------------------------------------------
// The drill (S3): every denial is EXERCISED, and the denial asserted
// -------------------------------------------------------------------------------------------

/// A probe's verdict, encoded as a child exit code so it survives `_exit`.
const PROBE_ALLOWED: i32 = 0;
const PROBE_DENIED: i32 = 1;
const PROBE_OTHER: i32 = 2;
/// The confinement itself failed to install in the probe child.
const PROBE_INSTALL_FAILED: i32 = 9;

fn classify_errno() -> i32 {
    match std::io::Error::last_os_error().raw_os_error() {
        Some(e) if e == libc::EACCES || e == libc::EPERM => PROBE_DENIED,
        _ => PROBE_OTHER,
    }
}

/// Fork a child, optionally confine it, run `probe`, and return the child's verdict.
///
/// The child performs raw syscalls only. Everything that allocates — the ruleset, the BPF program,
/// the C strings the probe uses — is built by the caller BEFORE the fork.
fn fork_probe(ruleset: Option<&OwnedFd>, bpf: Option<&[libc::sock_filter]>, probe: &dyn Fn() -> i32) -> Result<i32, String> {
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(format!("fork: {}", std::io::Error::last_os_error()));
    }
    if pid == 0 {
        let code = match unsafe { install(ruleset.map(|f| f.as_raw_fd()), bpf) } {
            Ok(()) => probe(),
            Err(_) => PROBE_INSTALL_FAILED,
        };
        unsafe { libc::_exit(code) };
    }
    let mut status: libc::c_int = 0;
    if unsafe { libc::waitpid(pid, &mut status, 0) } < 0 {
        return Err(format!("waitpid: {}", std::io::Error::last_os_error()));
    }
    if libc::WIFEXITED(status) {
        Ok(libc::WEXITSTATUS(status))
    } else {
        Err(format!("the probe child died on signal {}", libc::WTERMSIG(status)))
    }
}

fn probe_open_read(path: &CString) -> i32 {
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
    if fd < 0 {
        return classify_errno();
    }
    unsafe { libc::close(fd) };
    PROBE_ALLOWED
}

fn probe_open_write(path: &CString) -> i32 {
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_CLOEXEC, 0o600) };
    if fd < 0 {
        return classify_errno();
    }
    unsafe { libc::close(fd) };
    PROBE_ALLOWED
}

/// Reach a listener THIS process owns. A differential probe: the same connection is attempted
/// unconfined, so a refusal under the profile can never be mistaken for a refused port.
fn probe_connect(port: u16) -> i32 {
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return classify_errno();
    }
    let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    addr.sin_family = libc::AF_INET as libc::sa_family_t;
    addr.sin_port = port.to_be();
    addr.sin_addr.s_addr = 0x7f00_0001u32.to_be();
    let rc = unsafe {
        libc::connect(fd, &addr as *const libc::sockaddr_in as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_in>() as u32)
    };
    let verdict = if rc == 0 { PROBE_ALLOWED } else { classify_errno() };
    unsafe { libc::close(fd) };
    verdict
}

/// The kernel's own answer about this host, read and probed — never inferred from a version.
pub(super) struct KernelSupport {
    pub abi: i32,
    pub summary: String,
}

pub(super) fn kernel_supports_backend() -> Result<KernelSupport, String> {
    if !ARCH_SUPPORTED {
        return Err("the seccomp filter in this build is written for x86_64 and aarch64 only".into());
    }
    // The LSM list is the kernel saying which modules it BOOTED with — a Landlock compiled in but
    // left out of `lsm=` is a Landlock that is not there.
    let lsm = std::fs::read_to_string("/sys/kernel/security/lsm").ok().map(|s| s.trim().to_string());
    if let Some(list) = &lsm
        && !list.split(',').any(|m| m.trim() == "landlock")
    {
        return Err(format!("/sys/kernel/security/lsm is `{list}` and does not list landlock: this kernel's Landlock is not enabled"));
    }
    let abi = unsafe {
        libc::syscall(SYS_LANDLOCK_CREATE_RULESET, std::ptr::null::<RulesetAttrV1>(), 0usize, LANDLOCK_CREATE_RULESET_VERSION)
    };
    if abi <= 0 {
        return Err(format!(
            "landlock_create_ruleset(VERSION) answered {}: this kernel exposes no Landlock ABI ({})",
            abi,
            std::io::Error::last_os_error()
        ));
    }
    let mode = unsafe { libc::prctl(PR_GET_SECCOMP) };
    if mode < 0 {
        return Err(format!("prctl(PR_GET_SECCOMP): {} — this kernel has no seccomp filter mode", std::io::Error::last_os_error()));
    }
    Ok(KernelSupport {
        abi: abi as i32,
        summary: format!(
            "kernel: Landlock ABI {abi}, seccomp filter mode present, LSM list `{}`",
            lsm.unwrap_or_else(|| "unreadable (securityfs not mounted)".into())
        ),
    })
}

/// **The drill** (ADR-0079 S3, S12). Six legs, each one exercising a denial rather than the
/// absence of a crash. Any failing leg ⇒ `none` with the reason.
pub(super) fn establish(workdir: &Path, writable: &[PathBuf]) -> (Confinement, Vec<String>) {
    let mut notes = Vec::new();
    let support = match kernel_supports_backend() {
        Ok(support) => support,
        Err(why) => {
            notes.push(format!("{why}; reporting `none`"));
            return (Confinement::none(), notes);
        }
    };
    notes.push(support.summary);

    let plan = Plan::build(support.abi, workdir, writable);
    notes.extend(plan.notes());

    macro_rules! fail {
        ($($arg:tt)*) => {{
            notes.push(format!("drill FAILED: {}; reporting `none`", format_args!($($arg)*)));
            return (Confinement::none(), notes);
        }};
    }
    macro_rules! inconclusive {
        ($($arg:tt)*) => {{
            notes.push(format!("drill INCONCLUSIVE: {}; reporting `none`", format_args!($($arg)*)));
            return (Confinement::none(), notes);
        }};
    }

    // ---- (a) a dynamically linked child STARTS under the profile ---------------------------
    //
    // This is the leg that measures the system read set: a set narrowed below what the loader
    // needs aborts the child before `main`, and the backend then says `none` rather than shipping
    // a profile that kills every job.
    let Some(exec_probe) = EXEC_PROBE_CANDIDATES.iter().map(PathBuf::from).find(|p| p.is_file()) else {
        inconclusive!("no {} on this host, so the exec leg cannot run", EXEC_PROBE_CANDIDATES.join(" / "));
    };
    let ruleset = match plan.ruleset(Some(&exec_probe)) {
        Ok(fd) => fd,
        Err(why) => fail!("{why}"),
    };
    let exec_c = CString::new(exec_probe.as_os_str().as_encoded_bytes()).expect("a path with no NUL");
    let argv: [*const c_char; 2] = [exec_c.as_ptr(), std::ptr::null()];
    let envp: [*const c_char; 1] = [std::ptr::null()];
    let exec_leg = fork_probe(Some(&ruleset), Some(plan.bpf.as_slice()), &|| {
        unsafe { libc::execve(exec_c.as_ptr(), argv.as_ptr(), envp.as_ptr()) };
        PROBE_OTHER
    });
    match exec_leg {
        Ok(PROBE_ALLOWED) => {
            notes.push(format!("drill: a dynamically linked child ({}) starts under the profile", exec_probe.display()))
        }
        Ok(PROBE_INSTALL_FAILED) => fail!("the confinement did not install in the probe child"),
        Ok(code) => fail!("no child starts under the profile ({} exited {code}) — the read set is too narrow", exec_probe.display()),
        Err(why) => fail!("{why}"),
    }

    // ---- (b) a write inside the writable set LANDS, and one outside is DENIED ---------------
    let inside = workdir.join("palw-confinement-probe");
    let outside = std::env::temp_dir().join(format!("palw-confinement-probe-outside-{}", std::process::id()));
    if plan.write_set.iter().any(|w| outside.starts_with(w)) {
        inconclusive!("the outside-write probe {} is inside the writable set", outside.display());
    }
    let _ = std::fs::remove_file(&inside);
    let _ = std::fs::remove_file(&outside);
    let inside_c = CString::new(inside.as_os_str().as_encoded_bytes()).expect("a path with no NUL");
    let outside_c = CString::new(outside.as_os_str().as_encoded_bytes()).expect("a path with no NUL");
    let ruleset_b = match plan.ruleset(None) {
        Ok(fd) => fd,
        Err(why) => fail!("{why}"),
    };
    let wrote_inside = fork_probe(Some(&ruleset_b), Some(plan.bpf.as_slice()), &|| probe_open_write(&inside_c));
    let wrote_outside = fork_probe(Some(&ruleset_b), Some(plan.bpf.as_slice()), &|| probe_open_write(&outside_c));
    let _ = std::fs::remove_file(&inside);
    let _ = std::fs::remove_file(&outside);
    match (wrote_inside, wrote_outside) {
        (Ok(PROBE_ALLOWED), Ok(PROBE_DENIED)) => notes.push("drill: writes land inside the outbox and are denied outside it".into()),
        (inside_r, outside_r) => fail!(
            "write inside the allowed set {inside_r:?} (want Ok({PROBE_ALLOWED})) and write outside {outside_r:?} (want Ok({PROBE_DENIED}))"
        ),
    }

    // ---- (c) a READ outside the allowed set is DENIED — the half macOS could not deliver ----
    let mut read_leg_done = false;
    for candidate in READ_OUTSIDE_CANDIDATES {
        let path = PathBuf::from(candidate);
        if !path.is_file() || plan.read_set.iter().any(|r| path.starts_with(r)) {
            continue;
        }
        let c = CString::new(path.as_os_str().as_encoded_bytes()).expect("a path with no NUL");
        let unconfined = fork_probe(None, None, &|| probe_open_read(&c));
        if unconfined != Ok(PROBE_ALLOWED) {
            continue; // not readable even unconfined: it proves nothing.
        }
        match fork_probe(Some(&ruleset_b), Some(plan.bpf.as_slice()), &|| probe_open_read(&c)) {
            Ok(PROBE_DENIED) => {
                notes.push(format!("drill: {} is readable UNCONFINED and DENIED under the profile", path.display()));
                read_leg_done = true;
                break;
            }
            other => fail!("{} is readable unconfined and the profile did not deny it ({other:?})", path.display()),
        }
    }
    if !read_leg_done {
        inconclusive!("no file outside the read set was readable unconfined, so the read denial cannot be PROVEN");
    }

    // ---- (d) a read INSIDE the set still works: the profile is narrow, not empty ------------
    let readable = workdir.join("palw-confinement-read-inside");
    if std::fs::write(&readable, b"probe").is_err() {
        inconclusive!("cannot write the inside-read probe into {}", workdir.display());
    }
    let readable_c = CString::new(readable.as_os_str().as_encoded_bytes()).expect("a path with no NUL");
    let inside_read = fork_probe(Some(&ruleset_b), Some(plan.bpf.as_slice()), &|| probe_open_read(&readable_c));
    let _ = std::fs::remove_file(&readable);
    match inside_read {
        Ok(PROBE_ALLOWED) => notes.push("drill: a read inside the allowed set is permitted".into()),
        other => fail!("a read inside the allowed set was refused ({other:?}) — the profile denies what the job needs"),
    }

    // ---- (e) the network denial, proven against a listener this process owns ----------------
    let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:0") else {
        inconclusive!("cannot open a local listener to prove the network denial");
    };
    let Ok(addr) = listener.local_addr() else {
        inconclusive!("the probe listener has no address");
    };
    let port = addr.port();
    let unconfined = fork_probe(None, None, &|| probe_connect(port));
    let confined = fork_probe(Some(&ruleset_b), Some(plan.bpf.as_slice()), &|| probe_connect(port));
    drop(listener);
    if unconfined != Ok(PROBE_ALLOWED) {
        inconclusive!("the probe listener was unreachable even unconfined ({unconfined:?})");
    }
    match confined {
        Ok(PROBE_DENIED) => notes.push("drill: a socket reachable unconfined is DENIED under the profile".into()),
        other => fail!("the confined child reached a socket ({other:?})"),
    }

    // ---- (f) the stacked execve denial actually denies an exec ------------------------------
    //
    // The pre-exec filter CANNOT deny `execve` (it is installed before the worker's own exec), so
    // Decision 5's "no execve after setup" is a second filter the worker stacks on itself. This
    // leg proves the mechanism on this host: a child that starts confined, stacks the denial, and
    // then tries to exec the same program it just proved it could exec in leg (a).
    let after_exec = deny_filter(&denied_after_exec());
    let stacked = fork_probe(Some(&ruleset), Some(plan.bpf.as_slice()), &|| {
        if unsafe { libc::prctl(PR_GET_SECCOMP) } != SECCOMP_MODE_FILTER {
            return PROBE_OTHER;
        }
        if unsafe { stack_execve_denial(&after_exec) }.is_err() {
            return PROBE_INSTALL_FAILED;
        }
        unsafe { libc::execve(exec_c.as_ptr(), argv.as_ptr(), envp.as_ptr()) };
        classify_errno()
    });
    match stacked {
        Ok(PROBE_DENIED) => notes.push(format!(
            "drill: a child that stacks the execve denial cannot exec {} — the same program leg (a) proved it could",
            exec_probe.display()
        )),
        other => fail!("the stacked execve denial did not deny an exec ({other:?})"),
    }

    notes.push(
        "limitation, stated: the pre-exec filter cannot deny `execve` — it is installed before the worker's own exec. \
         The worker stacks that denial on itself by calling `confine_self_after_exec()` at the top of main; a worker \
         binary that does not call it keeps every other denial and loses only that one."
            .into(),
    );

    declare_backend_in_force(ConfinementBackend::LinuxSeccompLandlock);
    (Confinement::linux_seccomp_landlock(Arc::new(plan)), notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The BPF program has the shape a kernel will accept: an architecture check that kills, one
    /// comparison per denied syscall whose jump lands on the EPERM return, and an allow in between.
    #[test]
    fn the_filter_jumps_to_the_denial_and_not_past_it() {
        let denied = denied_syscalls();
        let prog = deny_filter(&denied);
        let arch_check = if cfg!(target_arch = "x86_64") { 6 } else { 4 };
        assert_eq!(prog.len(), denied.len() + arch_check + 2, "one comparison per denied syscall, plus the prologue and two returns");
        let eperm_at = prog.len() - 1;
        assert_eq!(prog[eperm_at].code, BPF_RET_K);
        assert_eq!(prog[eperm_at].k, SECCOMP_RET_ERRNO | libc::EPERM as u32);
        assert_eq!(prog[eperm_at - 1].k, SECCOMP_RET_ALLOW);
        for (i, _) in denied.iter().enumerate() {
            let at = arch_check + i;
            let target = at + 1 + prog[at].jt as usize;
            assert_eq!(target, eperm_at, "comparison {i} must jump to the EPERM return, not to {target}");
        }
    }

    /// The accesses a rule may carry are masked by what the ruleset handles AND by what the
    /// kernel accepts on a non-directory — a file rule carrying a directory-only right is EINVAL,
    /// which would take the whole ruleset down with it.
    #[test]
    fn a_file_rule_carries_only_file_accesses() {
        assert_eq!(FS_FILE_APPLICABLE & FS_READ_DIR, 0);
        assert_eq!(FS_FILE_APPLICABLE & FS_MAKE_REG, 0);
        assert_eq!(handled_access_fs(1) & FS_TRUNCATE, 0, "TRUNCATE is ABI 3");
        assert_eq!(handled_access_fs(1) & FS_REFER, 0, "REFER is ABI 2");
        assert_ne!(handled_access_fs(4) & FS_TRUNCATE, 0);
        assert_eq!(handled_access_fs(5) & (1 << 15), 0, "IOCTL_DEV is deliberately never handled");
    }
}
