//! **Pinned class artifacts: the one page-cache copy a host's seats share, kept resident and counted
//! once** (int-10.2 A1; `docs/design/palw/t12-replay-memory-1001.md`).
//!
//! # What was measured (testnet-12, 5.104, 2026-10-01)
//!
//! Five seats each mapped the 8k class's `.palwart` (1,716.0 MiB; 1,694.6 MiB of it int8 slabs read in
//! place, `Int8SlabV1::Mapped`). `smaps_rollup` showed the mapping as **1.68 G of `Shared_Clean` in
//! every seat** — one physical copy in the page cache, shared by all five — while every replay of the
//! class RESERVED the file's 1.68 GiB again on its seat's ledger (`holding_replay_bytes_v1`): five
//! seats, five charges for one copy, each against a 3.5 GiB share. And the copy was not safe in the
//! page cache either: under the host's pressure the kernel evicted it and the replays refaulted it 4 KiB
//! at a time at 6–11 MB/s (ADR-0112 §1's measurement of the fleet's virtio disks, against 845 MB/s for
//! large reads) — the minutes 5.104's replays spent.
//!
//! # What this does
//!
//! While a node loads its `--palw-class-artifact` list (`palw_backends::load_class_holdings_v1`), each
//! file whose replay reads a whole-file mapping IN PLACE — a dense `.palwart`, an IR container — is
//! pinned: mapped `PROT_READ | MAP_SHARED`, read into the page cache by large reads, and locked
//! (`misaka_palw_base0::mmap::PinnedFileMapV1`, whose doc says why its own mapping and why shared). Its
//! pages are then resident and unevictable for the process's life: the same pages the holding's own
//! mapping reads, one copy on the host however many processes lock it. A pinned holding is already
//! resident, so a replay's need no longer counts its bytes ([`holding_is_pinned_v1`] →
//! `incremental_replay_bytes_v1` answers 0, as it does for a Qwen3.6 residency) and no duty's share is
//! charged for them; they are reported instead — the load line, the periodic `[palw-host] memory`
//! line, and `getPalwNodeStatus.verification` (`pinned_mib`, `pinned_files`).
//!
//! # Why BEFORE the lineage maps the file
//!
//! The pin is taken ([`prepin_file_v1`]) before the lineage opens the file, and handed to the holding it
//! loads ([`settle_prepin_v1`]) after. Two reasons, both measured:
//!
//! * **The cold start.** The lineage's first pass over a dense file — the digest its decoder recomputes
//!   — faults the mapping in 4 KiB at a time: on the fleet's disks 1.7 GiB at 6–11 MB/s is minutes. The
//!   pin's large reads bring the same file in at hundreds of MB/s first, and the decoder then reads a
//!   resident page cache.
//! * **macOS refuses the shared lock afterwards.** There, once ANY process has faulted a file's pages
//!   through a `MAP_PRIVATE` mapping (the lineages' own, `ReadOnlyMap`), an `mlock` of a `MAP_SHARED`
//!   view of that file fails with `EPERM` — in the same process and across processes (this Mac,
//!   2026-10-01) — while a shared view locked first stays locked and lets private views read beside it.
//!   Locking the private view instead is not a substitute: macOS wires a private file mapping as this
//!   process's own copy (`footprint`: 257 MB for a locked 256 MB private view, 1 MB for the shared one).
//!   Linux has neither behaviour; the order costs it nothing.
//!
//! # What it never pins
//!
//! A holding with a residency of its own — the Qwen3.6 tier (ADR-0112), whose policy reads the
//! always-set and the routed experts into owned buffers and leaves the rest of a 33 GiB file to the
//! page cache, and any later residency of the IR tier — decides its own resident set, and pinning its
//! file would lock the 33 GiB its residency exists not to hold. Nor a derived class (no file), nor a
//! dense artifact a platform that cannot map decoded into owned memory. [`palw_pin_lineage_v1`] (before
//! the load) and [`palw_pin_eligibility_v1`] (after it) are the one list; a lineage not on it is not
//! pinned.
//!
//! # When it does not pin, and what then
//!
//! Every refusal keeps today's behaviour — the replay reserves the file's bytes, as before — and says
//! why with the numbers: pinning is off (`--palw-no-artifact-pin`); the file would take this process
//! past its cap (`--palw-artifact-pin-max-bytes`, default a quarter of the host's memory); the bytes not
//! yet resident (`mincore`) do not fit what the host and this process's memory cgroup can spare past
//! the node's 1 GiB reserve; the file the holding loaded is not the file that was locked; or the kernel
//! refused the lock — the warning then names `RLIMIT_MEMLOCK`, soft and hard, so an operator without
//! root knows what to raise (`LimitMEMLOCK=infinity` in a systemd unit, `ulimit -l unlimited` in a
//! shell).
//!
//! # Whose cgroup pays
//!
//! A page-cache page is charged to the memory cgroup of the process that FIRST faulted it in — on a
//! host of seats, the seat that loaded the artifact first. A pinned page stays there: it is in that
//! seat's `memory.current` for good, and therefore in the cgroup term of that seat's ledger
//! (`cgroup_headroom_from_v1`), which it lowers by the file; the other seats are charged nothing for it.
//! The periodic line says so beside `pinned_mib`, and the kit's `MemoryMax` floor already counts the
//! artifacts against every seat's limit (`check_memmax`).
//!
//! **Node-local, never consensus**: nothing here is read by the chain, and a node that pins nothing
//! judges every claim exactly as one that pins everything.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use kaspa_core::{info, warn};
use misaka_palw_base0::mmap::{FileIdentityV1, PinnedFileMapV1, memlock_rlimit_v1};
use misaka_palw_sdk::PalwLoadedArtifactV1;

/// **The node's pin policy**, armed once by the daemon (`--palw-no-artifact-pin`,
/// `--palw-artifact-pin-max-bytes`). Unarmed — a test, a tool that loads holdings without a daemon —
/// nothing is pinned: pinning is the node's decision, not a side effect of loading a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwArtifactPinPolicyV1 {
    pub enabled: bool,
    /// The most file bytes this process locks, over every artifact it pins.
    pub max_bytes: u64,
}

/// **The cap a node pins under when its operator names none: a quarter of the host's memory.** The
/// artifacts a seat reads in place today are 1.7 GiB each (the 8k `.palwart` and the IR container): a
/// 24 GiB fleet host pins both under its 6 GiB, an 8 GiB desktop the first under its 2 GiB. A host
/// running several seats pays for each distinct file ONCE, so the cap is per process and generous; the
/// spare-memory check is what refuses a pin the host cannot hold right now.
pub const PALW_ARTIFACT_PIN_DEFAULT_DIVISOR_V1: u64 = 4;

/// How large the reads that bring a file into the page cache before its lock are.
const PALW_ARTIFACT_PIN_READ_CHUNK_V1: usize = 8 << 20;

static POLICY: OnceLock<PalwArtifactPinPolicyV1> = OnceLock::new();

/// **Arm the pin policy**, once, from the daemon — before any service loads a holding. `max_bytes`
/// `None` is the default cap ([`PALW_ARTIFACT_PIN_DEFAULT_DIVISOR_V1`] of `total_memory`, unbounded
/// where the platform reports none). Arming twice keeps the first.
pub fn arm_artifact_pin_policy_v1(enabled: bool, max_bytes: Option<u64>, total_memory: u64) -> PalwArtifactPinPolicyV1 {
    let max_bytes = max_bytes.unwrap_or(if total_memory > 0 { total_memory / PALW_ARTIFACT_PIN_DEFAULT_DIVISOR_V1 } else { u64::MAX });
    *POLICY.get_or_init(|| PalwArtifactPinPolicyV1 { enabled, max_bytes })
}

/// The armed policy, or `None` before the daemon armed one.
pub fn armed_artifact_pin_policy_v1() -> Option<PalwArtifactPinPolicyV1> {
    POLICY.get().copied()
}

/// One pinned file: its identity, the holdings it pins (held weakly — the pin outlives no holding's
/// answer, and a payload address is never reused while a `Weak` to it lives), and the locked mapping,
/// whose length is the bytes it holds and whose drop releases them.
struct PinEntryV1 {
    path: PathBuf,
    identity: FileIdentityV1,
    holdings: Vec<Weak<dyn std::any::Any + Send + Sync>>,
    map: PinnedFileMapV1,
}

fn pins() -> &'static Mutex<Vec<PinEntryV1>> {
    static PINS: OnceLock<Mutex<Vec<PinEntryV1>>> = OnceLock::new();
    PINS.get_or_init(Default::default)
}

fn lock_pins() -> std::sync::MutexGuard<'static, Vec<PinEntryV1>> {
    pins().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// **Whether a file of this lineage is one a pin serves — asked BEFORE the file is mapped**, from the
/// lineage its magic dispatches to (`PalwClassSdk::lineage_id_for_file_v1`). `Err` names why not.
pub fn palw_pin_lineage_v1(lineage_id: &str) -> Result<(), &'static str> {
    match lineage_id {
        // The dense container is read in place where the platform maps it (whether it did is checked
        // on the holding, [`palw_pin_eligibility_v1`]); an IR container binds its params in place from
        // its mapping (`TirArtifactV1::open`). A residency for the IR tier, when one lands, answers
        // here as Qwen3.6 does below.
        misaka_palw_sdk::lineages::dense::DENSE_LINEAGE_ID | misaka_palw_sdk::lineages::tir::TIR_LINEAGE_ID_V1 => Ok(()),
        misaka_palw_sdk::lineages::qwen36::QWEN36_LINEAGE_ID => {
            Err("its own residency (ADR-0112) decides what of it is resident, and the rest is the page cache's")
        }
        _ => Err("its lineage does not read a whole-file mapping in place"),
    }
}

/// **Whether a LOADED holding is read in place from a whole-file mapping** — the pin's second check,
/// on what the lineage actually built. `Err` names why not.
pub fn palw_pin_eligibility_v1(holding: &PalwLoadedArtifactV1) -> Result<(), &'static str> {
    if holding.path.is_none() {
        return Err("it has no file (a derived class)");
    }
    palw_pin_lineage_v1(holding.lineage_id)?;
    if holding.lineage_id == misaka_palw_sdk::lineages::dense::DENSE_LINEAGE_ID {
        match misaka_palw_sdk::lineages::dense::artifact_of(holding) {
            Some(artifact) if artifact.embed.is_mapped() => {}
            _ => return Err("this platform decoded it into owned memory instead of mapping it"),
        }
    }
    Ok(())
}

/// **Whether this holding's file is pinned by this process** — the question the replay pricing asks
/// (`palw_backends::incremental_replay_bytes_v1`). By the holding's payload, not its path: a file
/// re-minted under the same name is another holding, and is not pinned until it is pinned.
pub fn holding_is_pinned_v1(holding: &PalwLoadedArtifactV1) -> bool {
    pinned_bytes_of_v1(holding) > 0
}

/// The bytes this process has pinned for `holding`'s file (0 when it is not pinned).
pub fn pinned_bytes_of_v1(holding: &PalwLoadedArtifactV1) -> u64 {
    let payload = holding.payload();
    let pins = lock_pins();
    pins.iter()
        .find(|pin| pin.holdings.iter().any(|weak| weak.upgrade().is_some_and(|held| Arc::ptr_eq(&held, &payload))))
        .map(|pin| pin.map.len() as u64)
        .unwrap_or(0)
}

/// What this process has pinned: how many files, how many bytes, and their identities — for the
/// telemetry and for the host's count of distinct pinned files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwPinnedSummaryV1 {
    pub files: usize,
    pub bytes: u64,
    pub identities: Vec<(PathBuf, FileIdentityV1)>,
}

pub fn pinned_summary_v1() -> PalwPinnedSummaryV1 {
    let pins = lock_pins();
    PalwPinnedSummaryV1 {
        files: pins.len(),
        bytes: pins.iter().fold(0u64, |acc, pin| acc.saturating_add(pin.map.len() as u64)),
        identities: pins.iter().map(|pin| (pin.path.clone(), pin.identity)).collect(),
    }
}

/// **Release the pins on these files** — the eviction path's twin (`palw_backends::evict_held_artifacts_v1`):
/// a holding released is a file this process no longer needs resident. Returns how many were released.
pub fn unpin_paths_v1(paths: &[PathBuf]) -> usize {
    let canonical: Vec<PathBuf> = paths.iter().map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())).collect();
    let mut pins = lock_pins();
    let before = pins.len();
    let host = crate::palw_host_ledger::shared_host_ledger_v1();
    pins.retain(|pin| {
        let keep = !canonical.contains(&pin.path);
        if !keep && let Some(host) = &host {
            host.unregister_pin(pin.identity);
        }
        keep
    });
    before - pins.len()
}

/// Release every pin this process holds.
pub fn unpin_all_v1() -> usize {
    let mut pins = lock_pins();
    let released = pins.len();
    if let Some(host) = crate::palw_host_ledger::shared_host_ledger_v1() {
        for pin in pins.iter() {
            host.unregister_pin(pin.identity);
        }
    }
    pins.clear();
    released
}

/// **Why a file was or was not pinned** — one per file, with the numbers its log line prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwPinOutcomeV1 {
    /// Locked: `resident` of its `bytes` were in the page cache already, the rest were read in.
    Pinned { bytes: u64, resident: u64, read_millis: u64 },
    /// This file is pinned already (the same file twice in one list); the holding joins that lock.
    AlreadyPinned { bytes: u64 },
    /// Not a file or a holding a pin serves ([`palw_pin_lineage_v1`], [`palw_pin_eligibility_v1`]).
    NotEligible(&'static str),
    /// Pinning is off, or this process has no armed policy.
    Disabled,
    /// It would take this process past its cap.
    OverCap { bytes: u64, cap: u64, pinned: u64 },
    /// The bytes not yet resident do not fit what the host can spare past the node's reserve.
    NoRoom { bytes: u64, not_resident: u64, spare: u64, reserve: u64 },
    /// The file did not open, read or load, or the holding's file is not the one locked.
    Changed(String),
    /// The kernel refused the lock.
    LockRefused { bytes: u64, error: String, rlimit: Option<(u64, u64)>, locked: u64 },
}

/// **A file locked before its holding exists** — [`prepin_file_v1`]'s product, handed to
/// [`settle_prepin_v1`] with the holding the lineage then loads. Dropped unsettled, it unlocks.
pub struct PalwPrepinV1 {
    canonical: PathBuf,
    map: PinnedFileMapV1,
    resident: u64,
    read_millis: u64,
}

/// **Lock `path` before its lineage maps it**, under the armed policy: `Ok` holds the lock for
/// [`settle_prepin_v1`]; `Err` is why not, which settles to today's behaviour. `lineage_id` is the
/// lineage the file's magic dispatches to; `spare` what the host and this process's cgroup can spare
/// now (`host_available_bytes_v1`; `None` where the platform cannot say, and the check is then the cap
/// alone).
pub fn prepin_file_v1(
    path: &Path,
    lineage_id: Result<&'static str, String>,
    spare: Option<u64>,
) -> Result<PalwPrepinV1, PalwPinOutcomeV1> {
    match armed_artifact_pin_policy_v1() {
        Some(policy) => prepin_file_with_policy_v1(path, lineage_id, spare, policy),
        None => Err(PalwPinOutcomeV1::Disabled),
    }
}

/// [`prepin_file_v1`] under a stated policy — what the armed one calls, and what a test calls
/// without arming the process (an armed policy would pin every other test's fixtures).
pub fn prepin_file_with_policy_v1(
    path: &Path,
    lineage_id: Result<&'static str, String>,
    spare: Option<u64>,
    policy: PalwArtifactPinPolicyV1,
) -> Result<PalwPrepinV1, PalwPinOutcomeV1> {
    if !policy.enabled {
        return Err(PalwPinOutcomeV1::Disabled);
    }
    let lineage_id = lineage_id.map_err(PalwPinOutcomeV1::Changed)?;
    palw_pin_lineage_v1(lineage_id).map_err(PalwPinOutcomeV1::NotEligible)?;
    let mut map =
        PinnedFileMapV1::map_shared(path).map_err(|e| PalwPinOutcomeV1::Changed(format!("it does not open for pinning: {e}")))?;
    let identity = map.identity();
    let bytes = identity.len;
    // The same file locked already (two names for one file in the list): no second lock, and the
    // holding joins the first at settlement.
    if lock_pins().iter().any(|pin| pin.identity == identity) {
        return Err(PalwPinOutcomeV1::AlreadyPinned { bytes });
    }
    let pinned = pinned_summary_v1().bytes;
    if pinned.saturating_add(bytes) > policy.max_bytes {
        return Err(PalwPinOutcomeV1::OverCap { bytes, cap: policy.max_bytes, pinned });
    }
    // A residency that cannot be read counts the whole file as not resident: the spare check is then
    // the only one that can be wrong, and it errs toward refusing.
    let resident = map.resident_bytes().unwrap_or(0).min(bytes);
    let not_resident = bytes - resident;
    let reserve = crate::palw_backends::PALW_REPLAY_HOST_RESERVE_BYTES_V1;
    if let Some(spare) = spare
        && not_resident > spare.saturating_sub(reserve)
    {
        return Err(PalwPinOutcomeV1::NoRoom { bytes, not_resident, spare, reserve });
    }
    let started = std::time::Instant::now();
    if not_resident > 0 {
        map.populate_by_reads(PALW_ARTIFACT_PIN_READ_CHUNK_V1)
            .map_err(|e| PalwPinOutcomeV1::Changed(format!("it could not be read whole: {e}")))?;
    }
    map.lock().map_err(|e| PalwPinOutcomeV1::LockRefused {
        bytes,
        error: e.to_string(),
        rlimit: memlock_rlimit_v1(),
        locked: pinned,
    })?;
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Ok(PalwPrepinV1 { canonical, map, resident, read_millis: started.elapsed().as_millis() as u64 })
}

/// **Hand a pre-taken lock to the holding the lineage loaded — or release it, and say why.** The lock
/// is kept only when the load succeeded, the holding is one a pin serves, and the file at the path is
/// still, by identity (device, inode, size, modification time), the one that was locked — so the
/// pages a replay will read are the pages held. `prepin` may already be a refusal; this settles it.
/// Logs the outcome under `role` and returns it.
pub fn settle_prepin_v1(
    role: &str,
    path: &Path,
    prepin: Result<PalwPrepinV1, PalwPinOutcomeV1>,
    loaded: &Result<PalwLoadedArtifactV1, String>,
) -> PalwPinOutcomeV1 {
    let outcome = settle_v1(path, prepin, loaded);
    let policy = armed_artifact_pin_policy_v1();
    // Nothing to say when the node never armed a policy (a tool, a test) or the load itself failed
    // (the load's own refusal is the line that matters).
    if policy.is_some() && loaded.is_ok() {
        log_outcome_v1(role, path, &outcome, policy.map_or(u64::MAX, |p| p.max_bytes));
    }
    outcome
}

fn settle_v1(
    path: &Path,
    prepin: Result<PalwPrepinV1, PalwPinOutcomeV1>,
    loaded: &Result<PalwLoadedArtifactV1, String>,
) -> PalwPinOutcomeV1 {
    let Ok(holding) = loaded else {
        return PalwPinOutcomeV1::Changed("it did not load".to_string());
    };
    let current = std::fs::File::open(path).and_then(|f| FileIdentityV1::of_file(&f)).ok();
    let payload = Arc::downgrade(&holding.payload());
    match prepin {
        Ok(prepin) => {
            if let Err(why) = palw_pin_eligibility_v1(holding) {
                return PalwPinOutcomeV1::NotEligible(why);
            }
            let identity = prepin.map.identity();
            if current != Some(identity) {
                return PalwPinOutcomeV1::Changed(format!(
                    "the file at the path ({current:?}) is not the one that was locked ({identity:?}) — it was replaced while it loaded"
                ));
            }
            let (bytes, resident, read_millis) = (identity.len, prepin.resident, prepin.read_millis);
            lock_pins().push(PinEntryV1 { path: prepin.canonical, identity, holdings: vec![payload], map: prepin.map });
            // The host counts each distinct pinned file once, whoever locks it (int-10.2 D1).
            if let Some(host) = crate::palw_host_ledger::shared_host_ledger_v1() {
                host.register_pin(identity);
            }
            PalwPinOutcomeV1::Pinned { bytes, resident, read_millis }
        }
        Err(PalwPinOutcomeV1::AlreadyPinned { bytes }) => {
            if let Err(why) = palw_pin_eligibility_v1(holding) {
                return PalwPinOutcomeV1::NotEligible(why);
            }
            let mut pins = lock_pins();
            match pins.iter_mut().find(|pin| Some(pin.identity) == current) {
                Some(pin) => {
                    pin.holdings.push(payload);
                    PalwPinOutcomeV1::AlreadyPinned { bytes }
                }
                None => PalwPinOutcomeV1::Changed("the file locked under another name is no longer the one at this path".to_string()),
            }
        }
        Err(other) => other,
    }
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1u64 << 30) as f64
}

/// `u64::MAX` reads as "unlimited" in a line an operator reads.
fn limit_text(bytes: u64) -> String {
    if bytes == u64::MAX { "unlimited".to_string() } else { format!("{:.2} MiB", bytes as f64 / (1u64 << 20) as f64) }
}

fn log_outcome_v1(role: &str, path: &Path, outcome: &PalwPinOutcomeV1, cap: u64) {
    let name = path.display();
    let keep = "replays of its class keep reserving the file's bytes, as before this release";
    match outcome {
        PalwPinOutcomeV1::Pinned { bytes, resident, read_millis } => {
            let summary = pinned_summary_v1();
            info!(
                "[{role}] pinned class artifact {name}: {:.2} GiB locked in RAM (its own PROT_READ|MAP_SHARED mapping, mlock; {:.2} GiB \
                 were resident already, the rest read in {read_millis} ms) — the host's one page-cache copy of it, shared by every \
                 process that maps it, is no longer evicted under pressure, and a replay of its class reserves none of its bytes. \
                 Pinned by this process: {} file(s), {:.2} GiB of a {} cap; the pages stay charged to the memory cgroup of whichever \
                 process faulted them in first",
                gib(*bytes),
                gib(*resident),
                summary.files,
                gib(summary.bytes),
                if cap == u64::MAX { "no".to_string() } else { format!("{:.2} GiB", gib(cap)) }
            );
        }
        // The same file under two names: the first line said everything.
        PalwPinOutcomeV1::AlreadyPinned { .. } => {}
        PalwPinOutcomeV1::NotEligible(why) => info!("[{role}] class artifact {name} is not pinned: {why}"),
        PalwPinOutcomeV1::Disabled => info!("[{role}] class artifact {name} is not pinned: --palw-no-artifact-pin; {keep}"),
        PalwPinOutcomeV1::OverCap { bytes, cap, pinned } => warn!(
            "[{role}] class artifact {name} ({:.2} GiB) is not pinned: with the {:.2} GiB this process has pinned it would pass the \
             {:.2} GiB cap (--palw-artifact-pin-max-bytes; the default is a quarter of the host's memory) — {keep}",
            gib(*bytes),
            gib(*pinned),
            gib(*cap)
        ),
        PalwPinOutcomeV1::NoRoom { bytes, not_resident, spare, reserve } => warn!(
            "[{role}] class artifact {name} ({:.2} GiB) is not pinned: {:.2} GiB of it are not resident and the host can spare {:.2} GiB \
             now, less the node's {:.2} GiB reserve — locking it would push this host toward swap. {keep}; it is pinned at the next \
             start the host has the room",
            gib(*bytes),
            gib(*not_resident),
            gib(*spare),
            gib(*reserve)
        ),
        PalwPinOutcomeV1::Changed(why) => warn!("[{role}] class artifact {name} is not pinned: {why}; {keep}"),
        PalwPinOutcomeV1::LockRefused { bytes, error, rlimit, locked } => warn!(
            "[{role}] class artifact {name} ({:.2} GiB) is not pinned: the kernel refused to lock it ({error}) with RLIMIT_MEMLOCK at {} \
             and {:.2} GiB already locked by this process — {keep}. Raise RLIMIT_MEMLOCK for this process (systemd: LimitMEMLOCK=infinity \
             in its unit; a shell: ulimit -l unlimited) or run it with CAP_IPC_LOCK; on macOS an EPERM here is the kernel refusing a \
             shared lock on a file another process already reads through a private mapping (start this node first, or stop the \
             other). --palw-no-artifact-pin turns pinning off",
            gib(*bytes),
            rlimit.map_or("an unreadable value".to_string(), |(soft, hard)| format!(
                "{} soft / {} hard",
                limit_text(soft),
                limit_text(hard)
            )),
            gib(*locked)
        ),
    }
}

/// The IR container's magic (`PALWTIR1`).
const PALW_PIN_TIR_MAGIC_V1: &[u8; 8] = b"PALWTIR1";

/// **The lineage a file's magic names, without loading it** — for the host pinner, which holds no SDK:
/// the IR container's and the Qwen3.6 container's own magic, and the dense container as everyone's
/// fallback (its decoder authenticates the format when a seat loads it).
pub fn palw_pin_lineage_of_file_v1(path: &Path) -> Result<&'static str, String> {
    let mut head = [0u8; 8];
    std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    // `misaka_palw_tir_artifact::PALW_TIR_CONTAINER_MAGIC_V1`, spelled here because the node does not link
    // the container crate; `the_pinners_magic_is_the_containers` holds the two together.
    Ok(if head == *PALW_PIN_TIR_MAGIC_V1 {
        misaka_palw_sdk::lineages::tir::TIR_LINEAGE_ID_V1
    } else if head == *misaka_palw_base0::qwen36::QWEN36_FILE_MAGIC {
        misaka_palw_sdk::lineages::qwen36::QWEN36_LINEAGE_ID
    } else {
        misaka_palw_sdk::lineages::dense::DENSE_LINEAGE_ID
    })
}

/// **The host pinner** (int-10.2 D1; `kaspad --palw-host-pinner`): a process that holds nothing but the
/// pins — every `--palw-class-artifact` a replay reads in place, locked by the same rules a node applies
/// (`prepin_file_with_policy_v1`: never a residency's container, under the cap, within the host's spare
/// memory) and registered in the host ledger when one is named — and then waits to be stopped.
///
/// **Why a process of its own** (the coordinator's condition 5): a page-cache page is charged to the
/// memory cgroup that FIRST faulted it in. Started before the seats (the kit's `misaka-palw-pinner`
/// unit, `Type=notify`, which the seats' units are ordered `After=`), the pinner is that cgroup: the
/// artifacts' pages sit in its `memory.current`, and no seat's ledger carries 1.7–3.4 GiB of a file in
/// its cgroup term because it happened to load first. Each seat still pins the same file for itself (its
/// own lock on the same pages — free: `mincore` reads them resident), so a seat outlives a stopped pinner
/// unchanged. On a host already running, the pages stay charged where they first were until that seat
/// restarts: its old cgroup's charge then passes to the parent slice, and the pinner's lock keeps the
/// pages resident, so the restarted seat faults none of them and is charged nothing.
///
/// **Readiness.** Once every file is locked, `READY=1` goes to systemd (`NOTIFY_SOCKET`; nothing without
/// one), so a seat ordered after the pinner starts only when the pages are the pinner's. A pinner that
/// locked nothing exits 1 — the unit's restart policy retries a host that had no room — rather than
/// report a readiness it does not have.
pub fn run_host_pinner_v1(paths: &[PathBuf], policy: PalwArtifactPinPolicyV1) -> ! {
    let host = crate::palw_host_ledger::shared_host_ledger_v1();
    if paths.is_empty() {
        warn!("[palw-pinner] no --palw-class-artifact to pin: nothing to hold (int-10.2 D1)");
        std::process::exit(1);
    }
    for path in paths {
        let spare = crate::palw_backends::host_available_bytes_v1();
        match prepin_file_with_policy_v1(path, palw_pin_lineage_of_file_v1(path), spare, policy) {
            Ok(prepin) => {
                let identity = prepin.map.identity();
                if let Some(host) = &host {
                    host.register_pin(identity);
                }
                info!(
                    "[palw-pinner] pinned {}: {:.2} GiB locked ({:.2} GiB were resident, the rest read in {} ms){}",
                    path.display(),
                    gib(identity.len),
                    gib(prepin.resident),
                    prepin.read_millis,
                    if host.is_some() { ", registered in the host ledger" } else { "" }
                );
                // Held by the registry, like a node's pins, for the life of the process; no holding names it.
                lock_pins().push(PinEntryV1 { path: prepin.canonical, identity, holdings: Vec::new(), map: prepin.map });
            }
            Err(outcome) => warn!("[palw-pinner] {} is not pinned: {outcome:?}", path.display()),
        }
    }
    let summary = pinned_summary_v1();
    if summary.files == 0 {
        warn!("[palw-pinner] none of the {} file(s) could be pinned (above): exiting so the unit retries (int-10.2 D1)", paths.len());
        std::process::exit(1);
    }
    let status = format!(
        "holding {} of {} file(s), {:.2} GiB, locked for this host's seats until stopped",
        summary.files,
        paths.len(),
        gib(summary.bytes)
    );
    match sd_notify_v1(&format!("READY=1\nSTATUS={status}")) {
        Ok(true) => info!("[palw-pinner] {status} (int-10.2 D1); systemd told READY"),
        Ok(false) => info!("[palw-pinner] {status} (int-10.2 D1)"),
        Err(e) => warn!("[palw-pinner] {status} (int-10.2 D1); systemd's NOTIFY_SOCKET refused READY ({e})"),
    }
    loop {
        std::thread::park();
    }
}

/// **`sd_notify(3)` without libsystemd**: one datagram to the socket systemd names in `NOTIFY_SOCKET`
/// (a path, or `@` + an abstract name on Linux). `Ok(false)`: no socket named — not under a
/// `Type=notify` unit, nothing to tell.
pub fn sd_notify_v1(state: &str) -> std::io::Result<bool> {
    match std::env::var_os("NOTIFY_SOCKET") {
        Some(socket) => sd_notify_to_v1(&socket, state).map(|()| true),
        None => Ok(false),
    }
}

/// [`sd_notify_v1`] to a named socket (what a test calls: it cannot set the process's environment
/// without racing its neighbours).
pub fn sd_notify_to_v1(socket: &std::ffi::OsStr, state: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::net::{SocketAddr, UnixDatagram};
        let bytes = socket.as_bytes();
        let addr = match bytes.first() {
            #[cfg(any(target_os = "linux", target_os = "android"))]
            Some(b'@') => {
                use std::os::linux::net::SocketAddrExt;
                SocketAddr::from_abstract_name(&bytes[1..])?
            }
            _ => SocketAddr::from_pathname(Path::new(socket))?,
        };
        UnixDatagram::unbound()?.send_to_addr(state.as_bytes(), &addr)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (socket, state);
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "no unix datagram sockets on this platform"))
    }
}

/// **The pin half of `getPalwNodeStatus.verification`**: `pinned_mib=<bytes this process locked>
/// pinned_files=<files>` — new keys in the existing `key=value` line, so the wire does not move.
pub fn pinned_status_v1() -> String {
    let summary = pinned_summary_v1();
    format!("pinned_mib={} pinned_files={}", summary.bytes >> 20, summary.files)
}

/// **The pin half of the periodic memory line.**
pub fn pinned_memory_note_v1() -> String {
    let summary = pinned_summary_v1();
    if summary.files == 0 {
        return "no class artifact pinned (pinned_mib=0)".to_string();
    }
    format!(
        "class artifacts pinned by this process: {:.2} GiB in {} file(s) (pinned_mib={}) — resident once on the host whoever locks \
         them, outside the reservation ledger, and charged to the memory cgroup of the process that first faulted each page in (that \
         process's memory.current, and so its ledger's cgroup term)",
        gib(summary.bytes),
        summary.files,
        summary.bytes >> 20
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY_ON: PalwArtifactPinPolicyV1 = PalwArtifactPinPolicyV1 { enabled: true, max_bytes: 1 << 30 };

    /// A dense `.palwart` on disk — a derived shape, encoded the way the converter writes one — in a
    /// file of its own.
    fn dense_fixture(name: &str) -> PathBuf {
        use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q, encode_artifact_file_v1};
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 2,
            n_kv_heads: 2,
            d_head: 4,
            d_ff: 12,
            vocab: 32,
            max_position: 16,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x9147).expect("a valid shape");
        let path = std::env::temp_dir().join(format!("misaka-pin-{name}-{}.palwart", std::process::id()));
        std::fs::write(&path, encode_artifact_file_v1(&artifact)).expect("write the fixture");
        path
    }

    /// The lineage's own load of a dense file: mapped, decoded in place.
    fn load_dense(path: &Path) -> Result<PalwLoadedArtifactV1, String> {
        let map = std::sync::Arc::new(misaka_palw_base0::mmap::ReadOnlyMap::open(path).map_err(|e| e.to_string())?);
        let mapped = misaka_palw_base0::artifact::decode_artifact_file_mapped_v1(map).map_err(|e| e.to_string())?;
        assert!(mapped.embed.is_mapped(), "the premise: the slabs are the mapping's pages");
        Ok(misaka_palw_sdk::lineages::dense::holding_from_artifact(std::sync::Arc::new(mapped), Some(path.to_path_buf())))
    }

    const DENSE: Result<&'static str, String> = Ok(misaka_palw_sdk::lineages::dense::DENSE_LINEAGE_ID);

    /// **A mapped dense file is locked before it loads, handed to its holding, counted by the holding's
    /// payload, and released with its eviction.** Under a stated policy (never the process's armed one,
    /// which would pin every other test's fixtures). The lock is taken first, the lineage's private view
    /// reads beside it, and settlement hands it over; `holding_is_pinned_v1` answers for this holding and
    /// for no other object over the same file until that one joins the same lock; the status counts it;
    /// an eviction releases it.
    #[test]
    fn a_mapped_dense_file_is_pinned_before_it_loads_and_released_with_its_eviction() {
        let path = dense_fixture("once");
        let bytes = std::fs::metadata(&path).unwrap().len();
        let prepin = prepin_file_with_policy_v1(&path, DENSE, None, POLICY_ON);
        let loaded = load_dense(&path);
        let holding = loaded.as_ref().expect("loads").clone();
        assert!(!holding_is_pinned_v1(&holding), "not until it is settled");
        match settle_prepin_v1("palw-test", &path, prepin, &loaded) {
            PalwPinOutcomeV1::Pinned { bytes: b, resident, .. } => assert!(b == bytes && resident <= bytes),
            other => panic!("pinned: {other:?}"),
        }
        assert!(holding_is_pinned_v1(&holding));
        assert_eq!(pinned_bytes_of_v1(&holding), bytes);
        assert!(pinned_status_v1().contains("pinned_files="));
        // Another holding of the same file (a second name, or a reload): the file is locked already, so
        // the lock is not taken twice and the new holding joins it at settlement.
        let again = prepin_file_with_policy_v1(&path, DENSE, None, POLICY_ON);
        assert!(matches!(again, Err(PalwPinOutcomeV1::AlreadyPinned { bytes: b }) if b == bytes));
        let second = load_dense(&path);
        let stranger = second.as_ref().expect("loads").clone();
        assert!(!holding_is_pinned_v1(&stranger), "pinned by its payload, not its path");
        assert_eq!(settle_prepin_v1("palw-test", &path, again, &second), PalwPinOutcomeV1::AlreadyPinned { bytes });
        assert!(holding_is_pinned_v1(&stranger));
        assert_eq!(unpin_paths_v1(std::slice::from_ref(&path)), 1, "one file, one lock, released");
        assert!(!holding_is_pinned_v1(&holding) && !holding_is_pinned_v1(&stranger));
        std::fs::remove_file(&path).ok();
    }

    /// **The host pinner reads a file's lineage from its magic exactly as the SDK dispatches it** — the
    /// IR container's `PALWTIR1` (spelled in this module, held to the container crate's constant here),
    /// the Qwen3.6 container's own magic (never pinned), and the dense container as everyone's fallback.
    #[test]
    fn the_pinners_magic_is_the_containers() {
        assert_eq!(PALW_PIN_TIR_MAGIC_V1, misaka_palw_tir_artifact::PALW_TIR_CONTAINER_MAGIC_V1);
        let dir = std::env::temp_dir().join(format!("misaka-pin-magic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (head, want) in [
            (*PALW_PIN_TIR_MAGIC_V1, misaka_palw_sdk::lineages::tir::TIR_LINEAGE_ID_V1),
            (*misaka_palw_base0::qwen36::QWEN36_FILE_MAGIC, misaka_palw_sdk::lineages::qwen36::QWEN36_LINEAGE_ID),
            (*b"PALWB0A2", misaka_palw_sdk::lineages::dense::DENSE_LINEAGE_ID),
        ] {
            let path = dir.join(want);
            std::fs::write(&path, [head.as_slice(), &[0u8; 8]].concat()).unwrap();
            assert_eq!(palw_pin_lineage_of_file_v1(&path), Ok(want));
            assert_eq!(palw_pin_lineage_v1(want).is_ok(), want != misaka_palw_sdk::lineages::qwen36::QWEN36_LINEAGE_ID);
        }
        assert!(palw_pin_lineage_of_file_v1(&dir.join("absent")).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **The pinner's readiness reaches the socket systemd names**: one datagram, `READY=1` and the
    /// status, read back from a socket bound where `NOTIFY_SOCKET` would point (the abstract form is
    /// Linux's and is spelled the same way).
    #[cfg(unix)]
    #[test]
    fn the_pinners_readiness_reaches_the_notify_socket() {
        let path = std::env::temp_dir().join(format!("misaka-notify-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixDatagram::bind(&path).expect("a socket where systemd's would be");
        sd_notify_to_v1(path.as_os_str(), "READY=1\nSTATUS=holding 2 of 2 file(s)").expect("sent");
        let mut buf = [0u8; 128];
        let n = listener.recv(&mut buf).expect("received");
        assert_eq!(&buf[..n], b"READY=1\nSTATUS=holding 2 of 2 file(s)");
        assert!(sd_notify_to_v1(std::ffi::OsStr::new("/nonexistent/notify.sock"), "READY=1").is_err(), "no socket there");
        std::fs::remove_file(&path).ok();
    }

    /// **Every refusal keeps today's behaviour and names its numbers**: pinning off, a residency's
    /// lineage, a cap the file passes, bytes not resident that the host cannot spare past the reserve, a
    /// file replaced between the lock and the load, and a holding that is not a mapping (a derived one)
    /// — none is pinned, and an unsettled lock unlocks.
    #[test]
    fn a_file_is_not_pinned_past_the_policy_the_cap_the_spare_memory_or_a_replacement() {
        let path = dense_fixture("refused");
        let bytes = std::fs::metadata(&path).unwrap().len();
        let off = PalwArtifactPinPolicyV1 { enabled: false, max_bytes: u64::MAX };
        assert!(matches!(prepin_file_with_policy_v1(&path, DENSE, None, off), Err(PalwPinOutcomeV1::Disabled)));
        let qwen36 = Ok(misaka_palw_sdk::lineages::qwen36::QWEN36_LINEAGE_ID);
        assert!(
            matches!(prepin_file_with_policy_v1(&path, qwen36, None, POLICY_ON), Err(PalwPinOutcomeV1::NotEligible(why)) if why.contains("residency")),
            "a residency's container is never pinned"
        );
        let capped = PalwArtifactPinPolicyV1 { enabled: true, max_bytes: bytes - 1 };
        assert!(
            matches!(prepin_file_with_policy_v1(&path, DENSE, None, capped), Err(PalwPinOutcomeV1::OverCap { bytes: b, cap, .. }) if b == bytes && cap == bytes - 1)
        );
        // No room: a spare of nothing refuses any byte not resident; a file wholly resident needs no
        // new memory to lock, and is locked. Either way the arithmetic is the one stated.
        match prepin_file_with_policy_v1(&path, DENSE, Some(0), POLICY_ON) {
            Err(PalwPinOutcomeV1::NoRoom { not_resident, spare, reserve, .. }) => {
                assert!(not_resident > 0 && spare == 0 && reserve == crate::palw_backends::PALW_REPLAY_HOST_RESERVE_BYTES_V1)
            }
            Ok(prepin) => assert_eq!(prepin.resident, bytes, "only a resident file locks with no spare"),
            Err(other) => panic!("{other:?}"),
        }
        // Replaced between the lock and the load: the lock is released, nothing is pinned.
        let prepin = prepin_file_with_policy_v1(&path, DENSE, None, POLICY_ON);
        assert!(prepin.is_ok(), "{:?}", prepin.as_ref().err());
        let staged = dense_fixture("refused-staged");
        let mut grown = std::fs::read(&staged).unwrap();
        grown.extend_from_slice(&[0u8; 7]);
        std::fs::write(&staged, &grown).unwrap();
        std::fs::rename(&staged, &path).expect("replaced by rename");
        let replaced = Ok(misaka_palw_sdk::lineages::dense::holding_from_artifact(
            misaka_palw_sdk::lineages::dense::artifact_of(&load_dense(&dense_fixture("refused-other")).unwrap()).unwrap(),
            Some(path.clone()),
        ));
        let outcome = settle_prepin_v1("palw-test", &path, prepin, &replaced);
        assert!(matches!(&outcome, PalwPinOutcomeV1::Changed(why) if why.contains("replaced while it loaded")), "{outcome:?}");
        assert!(!holding_is_pinned_v1(replaced.as_ref().unwrap()));
        // A holding with no file — the derived floor's shape — is never pinned, whatever was locked.
        let prepin = prepin_file_with_policy_v1(&path, DENSE, None, POLICY_ON);
        let derived = Ok(misaka_palw_sdk::lineages::dense::holding_from_artifact(
            misaka_palw_sdk::lineages::dense::artifact_of(replaced.as_ref().unwrap()).unwrap(),
            None,
        ));
        assert_eq!(
            settle_prepin_v1("palw-test", &path, prepin, &derived),
            PalwPinOutcomeV1::NotEligible("it has no file (a derived class)")
        );
        assert_eq!(pinned_summary_v1().identities.iter().filter(|(p, _)| p.ends_with(path.file_name().unwrap())).count(), 0);
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(std::env::temp_dir().join(format!("misaka-pin-refused-other-{}.palwart", std::process::id()))).ok();
    }
}
