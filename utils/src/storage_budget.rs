//! **The node's storage budget: one managed root, one byte cap, per-category budgets, reservations
//! taken before large writes, reclaim before use, and a clean low-space stop** (G14 capacity lane,
//! `docs/design/palw/storage/capacity.md`).
//!
//! Node-local and default OFF: nothing here is read by a consensus rule, and a node started without
//! `--storage-cap-bytes` never constructs a [`StorageBudget`], so every call site that asks
//! [`global`] gets `None` and behaves exactly as before.
//!
//! # The model
//!
//! * **Charge.** Every byte under the managed root (and any extra path registered with it, e.g. an
//!   external RocksDB WAL dir) is assigned to one [`StorageCategory`] by the longest registered path
//!   prefix (unmatched bytes under the root are [`StorageCategory::Other`]). A periodic scan
//!   ([`StorageBudget::refresh`]) measures the ALLOCATED size on disk. A category's charge combines
//!   the scan with its outstanding reservations according to its [`ReservationMode`]:
//!   - `Envelope` (staging DB, IBD spill): the reservation is an upper bound on everything the
//!     category will hold, so the charge is `max(scanned, reserved)` — a staging DB that grows into
//!     its reservation is not counted twice.
//!   - `Additive` (everything else): a reservation covers bytes NOT YET on disk (a file about to be
//!     written, a log rotation's compressed copy), so the charge is `scanned + reserved` — while the
//!     write is in progress its bytes count twice, which errs on the safe side.
//! * **Reservation.** [`StorageBudget::reserve`] refuses when `cap − safety_margin − charge < bytes`
//!   (reservations never eat into the safety margin; only organic growth of live state can), when the
//!   category would exceed its own budget, or when the physical volume (minus its margin and every
//!   outstanding not-yet-materialised reservation) has less than `bytes` free. The returned
//!   [`StorageReservation`] releases on drop. Concurrent tasks therefore never count the same free
//!   bytes twice.
//! * **Reclaim before use.** [`StorageBudget::reserve_after_reclaim`] first runs every registered
//!   [`StorageReclaimer`] in [`ReclaimMode::Obsolete`] (data nothing can use any more: orphaned staging
//!   DB dirs, abandoned temp files, log archives beyond the rotation window), rescans, and only if the
//!   reservation is still refused runs them again in [`ReclaimMode::UnderPressure`] (re-makeable data
//!   whose obligations have ended, oldest first). **A reclaimer never deletes material the node is
//!   still liable for**; when that is all that is left, the reservation is refused and the caller
//!   waits — space shortage is never converted into deleting evidence.
//! * **Low-space stop.** After every refresh, when the headroom (the smaller of cap headroom and
//!   physical headroom) falls under `safety_margin`, the budget first reclaims under pressure, and if
//!   that is not enough it enters the PAUSED state: every registered pause hook runs (the node's hooks
//!   fsync the RocksDB WALs), [`StorageBudget::is_paused`] turns true and intake that writes
//!   (IBD, relay block intake, RPC block submission) waits in [`StorageBudget::wait_until_writable`].
//!   It resumes when the headroom is back above `resume_margin` (hysteresis). Pausing is an intake
//!   decision upstream of consensus: no block is ever judged, and none is marked invalid, because of it.
//!
//! The budget is a measured, conservative bound — not a filesystem quota. The managed root should sit
//! on a dedicated quota-capable volume (XFS project quota) so the kernel enforces the cap the budget
//! plans against.
use parking_lot::Mutex;
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// The cap the 30 GB pruned-node policy recommends (`--storage-cap-bytes=30000000000`).
pub const RECOMMENDED_STORAGE_CAP_BYTES: u64 = 30_000_000_000;

/// What a byte on disk is for. The index is the position in every per-category array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StorageCategory {
    /// The active consensus DB, the meta DB and the UTXO index.
    ActiveDb = 0,
    /// The staging consensus DB an IBD-with-headers-proof builds next to the active one.
    StagingDb = 1,
    /// RocksDB write-ahead logs, info logs and compaction headroom of the active DBs.
    WalMaintenance = 2,
    /// PALW retained material (`palw-retention/`, `palw-panel/`). A reclaim TARGET: never a reason
    /// to delete material the node is still liable for.
    PalwRetained = 3,
    /// IBD / snapshot receive-and-unpack spill files.
    IbdSpill = 4,
    /// The node's own log files.
    Logs = 5,
    /// Everything else under the managed root (reports, peer stores, management data).
    Other = 6,
}

pub const STORAGE_CATEGORY_COUNT: usize = 7;

impl StorageCategory {
    pub const ALL: [StorageCategory; STORAGE_CATEGORY_COUNT] = [
        StorageCategory::ActiveDb,
        StorageCategory::StagingDb,
        StorageCategory::WalMaintenance,
        StorageCategory::PalwRetained,
        StorageCategory::IbdSpill,
        StorageCategory::Logs,
        StorageCategory::Other,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            StorageCategory::ActiveDb => "active-db",
            StorageCategory::StagingDb => "staging-db",
            StorageCategory::WalMaintenance => "wal-maintenance",
            StorageCategory::PalwRetained => "palw-retained",
            StorageCategory::IbdSpill => "ibd-spill",
            StorageCategory::Logs => "logs",
            StorageCategory::Other => "other",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    /// How a reservation in this category combines with what the scan sees (see the module docs).
    pub fn reservation_mode(self) -> ReservationMode {
        match self {
            StorageCategory::StagingDb | StorageCategory::IbdSpill => ReservationMode::Envelope,
            _ => ReservationMode::Additive,
        }
    }
}

impl fmt::Display for StorageCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReservationMode {
    /// charge = max(scanned, reserved): the reservation bounds everything the category will hold.
    Envelope,
    /// charge = scanned + reserved: the reservation covers bytes not yet on disk.
    Additive,
}

/// The initial DESIGN budget for a 30 GB cap (the G14 brief; not measured — adjust from
/// measurements). Sums to exactly 30,000,000,000.
pub const DESIGN_BUDGET_30GB: [u64; STORAGE_CATEGORY_COUNT] = [
    10_497_346_816, // active DBs
    10_497_346_816, // staging DBs
    4_000_000_000,  // WAL / compaction / maintenance
    2_000_000_000,  // PALW retained material (reclaim target)
    805_306_368,    // IBD spill: 384 MiB × 2 sessions
    2_000_000_000,  // logs
    200_000_000,    // other
];

/// The per-category budgets for `cap`: the 30 GB design budget scaled proportionally (so a 50 MB
/// test cap keeps the same shape). Operators override single categories on top.
pub fn budgets_for_cap(cap: u64) -> [u64; STORAGE_CATEGORY_COUNT] {
    let mut out = [0u64; STORAGE_CATEGORY_COUNT];
    for (i, design) in DESIGN_BUDGET_30GB.iter().enumerate() {
        out[i] = ((*design as u128 * cap as u128) / RECOMMENDED_STORAGE_CAP_BYTES as u128) as u64;
    }
    out
}

/// The safety margin used when the operator does not name one: 1 % of the cap, at least 64 MiB
/// (but never more than an eighth of the cap, so tiny test caps stay usable).
pub fn default_safety_margin(cap: u64) -> u64 {
    (cap / 100).max(64 << 20).min(cap / 8)
}

#[derive(Clone, Debug)]
pub struct StorageBudgetConfig {
    /// The managed root (`<appdir>/<network>`).
    pub root: PathBuf,
    pub cap_bytes: u64,
    pub budgets: [u64; STORAGE_CATEGORY_COUNT],
    /// Under this much headroom the node pauses write intake; reservations never use it.
    pub safety_margin_bytes: u64,
    /// The node resumes once headroom is back above this (≥ safety margin; hysteresis).
    pub resume_margin_bytes: u64,
    /// Free bytes the PHYSICAL volume must keep besides the cap's own margin (another tenant, the
    /// filesystem's own reserve).
    pub physical_margin_bytes: u64,
}

impl StorageBudgetConfig {
    pub fn for_cap(root: PathBuf, cap_bytes: u64) -> Self {
        let safety = default_safety_margin(cap_bytes);
        Self {
            root,
            cap_bytes,
            budgets: budgets_for_cap(cap_bytes),
            safety_margin_bytes: safety,
            resume_margin_bytes: safety.saturating_mul(2).min(cap_bytes / 4).max(safety),
            physical_margin_bytes: safety,
        }
    }

    /// Problems an operator should hear about (not refusals): budgets that do not fit the cap, a
    /// margin that leaves nothing usable.
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        let sum: u128 = self.budgets.iter().map(|b| *b as u128).sum();
        if sum > self.cap_bytes as u128 {
            out.push(format!(
                "the per-category budgets sum to {sum} bytes, above the cap of {} bytes; the cap is what is enforced",
                self.cap_bytes
            ));
        }
        if self.safety_margin_bytes >= self.cap_bytes {
            out.push(format!(
                "the safety margin {} leaves no usable space under the cap {}",
                self.safety_margin_bytes, self.cap_bytes
            ));
        }
        if self.resume_margin_bytes < self.safety_margin_bytes {
            out.push("the resume margin is below the safety margin; it is raised to it".to_string());
        }
        out
    }
}

/// Why a reservation was refused. Never a statement about any block or peer: the node lacks space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRefusal {
    pub category: StorageCategory,
    pub requested: u64,
    pub reason: StorageRefusalReason,
    /// The headroom that was available under the limit that refused.
    pub available: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageRefusalReason {
    /// `cap − safety_margin − charge < requested`.
    Cap,
    /// The category's own budget would be exceeded.
    CategoryBudget,
    /// The physical volume has less free space than requested (after its margin and pending reservations).
    PhysicalVolume,
    /// The node is in the low-space PAUSED state.
    Paused,
}

impl fmt::Display for StorageRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let why = match self.reason {
            StorageRefusalReason::Cap => "the storage cap",
            StorageRefusalReason::CategoryBudget => "the category budget",
            StorageRefusalReason::PhysicalVolume => "the physical volume",
            StorageRefusalReason::Paused => "the low-space pause",
        };
        write!(
            f,
            "local storage: {} bytes for {} refused by {why} ({} bytes available) — a node-local capacity condition, not a validation result",
            self.requested, self.category, self.available
        )
    }
}

impl std::error::Error for StorageRefusal {}

/// How hard a reclaimer may work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReclaimMode {
    /// Only data nothing can use any more (orphans, abandoned temp files, logs beyond the window).
    Obsolete,
    /// Also re-makeable data whose obligations have ended, oldest first, until `need` bytes are free.
    UnderPressure { need: u64 },
}

/// Something that can give bytes back. Implementations MUST NOT delete material the node is still
/// liable for (unresolved claim / court / DA material, permanent registrations), and MUST treat
/// "could not determine whether it is still owed" as owed.
pub trait StorageReclaimer: Send + Sync {
    fn name(&self) -> &str;
    /// Reclaim, returning the bytes it believes it freed (the next scan is authoritative).
    fn reclaim(&self, mode: ReclaimMode) -> u64;
}

/// A reclaimer from a closure (the node wires its GC passes with these).
pub struct FnReclaimer<F: Fn(ReclaimMode) -> u64 + Send + Sync> {
    name: String,
    f: F,
}

impl<F: Fn(ReclaimMode) -> u64 + Send + Sync> FnReclaimer<F> {
    pub fn new(name: impl Into<String>, f: F) -> Arc<Self> {
        Arc::new(Self { name: name.into(), f })
    }
}

impl<F: Fn(ReclaimMode) -> u64 + Send + Sync> StorageReclaimer for FnReclaimer<F> {
    fn name(&self) -> &str {
        &self.name
    }
    fn reclaim(&self, mode: ReclaimMode) -> u64 {
        (self.f)(mode)
    }
}

/// **Old logs** (G14 reclaim): rotated archives of the node's own log files. In
/// [`ReclaimMode::Obsolete`] only archives beyond the rotation window (`<name>.<n>.gz` with
/// `n > keep`, left by a build with a wider window) and files ending `.partial`/`.tmp`; under
/// pressure, the remaining archives oldest-index first. The active log file is never touched.
pub fn reclaim_log_archives(dir: &Path, keep: u32, mode: ReclaimMode) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut archives: Vec<(u32, PathBuf, u64)> = Vec::new();
    let mut freed = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if !meta.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let bytes = allocated_bytes(&meta);
        if name.ends_with(".tmp") || name.ends_with(".partial") {
            if std::fs::remove_file(&path).is_ok() {
                freed = freed.saturating_add(bytes);
            }
            continue;
        }
        // `<base>.log.<n>.gz`
        let Some(stem) = name.strip_suffix(".gz") else { continue };
        let Some((base, index)) = stem.rsplit_once('.') else { continue };
        let Ok(index) = index.parse::<u32>() else { continue };
        if !base.ends_with(".log") {
            continue;
        }
        if index > keep {
            if std::fs::remove_file(&path).is_ok() {
                freed = freed.saturating_add(bytes);
            }
        } else {
            archives.push((index, path, bytes));
        }
    }
    if let ReclaimMode::UnderPressure { need } = mode {
        // Highest index = oldest archive first.
        archives.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, path, bytes) in archives {
            if freed >= need {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                freed = freed.saturating_add(bytes);
            }
        }
    }
    freed
}

/// **Abandoned temp files** in the managed temp dir: older than `horizon` (live spill files are
/// unlinked at creation on unix, so anything visible there is left by a crash or another platform).
pub fn reclaim_temp_dir(dir: &Path, horizon: Duration) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut freed = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        let old = meta.modified().ok().and_then(|t| t.elapsed().ok()).is_none_or(|age| age >= horizon);
        if !old {
            continue;
        }
        let bytes = if meta.is_dir() { disk_usage(&path) } else { allocated_bytes(&meta) };
        let removed = if meta.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        if removed.is_ok() {
            freed = freed.saturating_add(bytes);
        }
    }
    freed
}

/// Free bytes of the physical volume holding a path (`None` = unknown, treated as unlimited).
pub type FreeSpaceProbe = Arc<dyn Fn(&Path) -> Option<u64> + Send + Sync>;

/// One measurement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StorageSnapshot {
    pub scanned: [u64; STORAGE_CATEGORY_COUNT],
    pub reserved: [u64; STORAGE_CATEGORY_COUNT],
    pub charge: [u64; STORAGE_CATEGORY_COUNT],
    pub charge_total: u64,
    pub cap_bytes: u64,
    pub physical_free: Option<u64>,
    /// min(cap headroom, physical headroom), before the safety margin.
    pub headroom: u64,
    pub paused: bool,
}

impl StorageSnapshot {
    pub fn summary_line(&self) -> String {
        let cats =
            StorageCategory::ALL.iter().map(|c| format!("{}={}", c.name(), self.charge[c.index()])).collect::<Vec<_>>().join(" ");
        format!(
            "storage: {} of {} bytes charged (headroom {}{}), {}",
            self.charge_total,
            self.cap_bytes,
            self.headroom,
            if self.paused { ", PAUSED" } else { "" },
            cats
        )
    }
}

#[derive(Default)]
struct State {
    scanned: [u64; STORAGE_CATEGORY_COUNT],
    reserved: [u64; STORAGE_CATEGORY_COUNT],
    physical_free: Option<u64>,
    paused: bool,
    paused_since: Option<Instant>,
}

type PauseHook = Box<dyn Fn() + Send + Sync>;

pub struct StorageBudget {
    config: StorageBudgetConfig,
    state: Mutex<State>,
    /// Longest prefix first.
    paths: Mutex<Vec<(PathBuf, StorageCategory)>>,
    reclaimers: Mutex<Vec<Arc<dyn StorageReclaimer>>>,
    pause_hooks: Mutex<Vec<PauseHook>>,
    probe: FreeSpaceProbe,
    resumed: event_listener::Event,
    paused_flag: AtomicBool,
}

impl StorageBudget {
    /// A budget probing the real volume (sysinfo's mount list).
    pub fn new(config: StorageBudgetConfig) -> Arc<Self> {
        Self::with_probe(config, Arc::new(volume_free_bytes))
    }

    /// A budget with an injected free-space probe (tests scale the volume down with it).
    pub fn with_probe(mut config: StorageBudgetConfig, probe: FreeSpaceProbe) -> Arc<Self> {
        config.resume_margin_bytes = config.resume_margin_bytes.max(config.safety_margin_bytes);
        let root = config.root.clone();
        Arc::new(Self {
            config,
            state: Mutex::new(State::default()),
            paths: Mutex::new(vec![(root, StorageCategory::Other)]),
            reclaimers: Mutex::new(Vec::new()),
            pause_hooks: Mutex::new(Vec::new()),
            probe,
            resumed: event_listener::Event::new(),
            paused_flag: AtomicBool::new(false),
        })
    }

    pub fn config(&self) -> &StorageBudgetConfig {
        &self.config
    }

    pub fn root(&self) -> &Path {
        &self.config.root
    }

    /// Assign every byte under `path` to `category` (longest prefix wins). Replaces an earlier
    /// assignment of the same path.
    pub fn assign_path(&self, path: impl Into<PathBuf>, category: StorageCategory) {
        let path = path.into();
        let mut paths = self.paths.lock();
        paths.retain(|(p, _)| *p != path);
        paths.push((path, category));
        paths.sort_by_key(|(p, _)| std::cmp::Reverse(p.as_os_str().len()));
    }

    /// Forget an assignment (its bytes fall back to the next-longest prefix).
    pub fn unassign_path(&self, path: &Path) {
        if path == self.config.root {
            return;
        }
        self.paths.lock().retain(|(p, _)| p != path);
    }

    pub fn register_reclaimer(&self, reclaimer: Arc<dyn StorageReclaimer>) {
        self.reclaimers.lock().push(reclaimer);
    }

    /// Run when the node enters the low-space pause (the node's hooks fsync its DB WALs).
    pub fn add_pause_hook(&self, hook: impl Fn() + Send + Sync + 'static) {
        self.pause_hooks.lock().push(Box::new(hook));
    }

    pub fn is_paused(&self) -> bool {
        self.paused_flag.load(Ordering::Acquire)
    }

    /// Wait until the node is out of the low-space pause. Returns at once when not paused.
    pub async fn wait_until_writable(&self) {
        loop {
            if !self.is_paused() {
                return;
            }
            let listener = self.resumed.listen();
            if !self.is_paused() {
                return;
            }
            listener.await;
        }
    }

    /// Blocking variant with a timeout; `true` when writable.
    pub fn wait_until_writable_blocking(&self, timeout: Duration) -> bool {
        if !self.is_paused() {
            return true;
        }
        let listener = self.resumed.listen();
        if !self.is_paused() {
            return true;
        }
        listener.wait_timeout(timeout);
        !self.is_paused()
    }

    fn charges(&self, state: &State) -> ([u64; STORAGE_CATEGORY_COUNT], u64) {
        let mut charge = [0u64; STORAGE_CATEGORY_COUNT];
        let mut total = 0u64;
        for c in StorageCategory::ALL {
            let i = c.index();
            charge[i] = match c.reservation_mode() {
                ReservationMode::Envelope => state.scanned[i].max(state.reserved[i]),
                ReservationMode::Additive => state.scanned[i].saturating_add(state.reserved[i]),
            };
            total = total.saturating_add(charge[i]);
        }
        (charge, total)
    }

    /// Bytes reserved but not yet visible to the scan — what the physical volume must still provide.
    fn pending(&self, state: &State) -> u64 {
        StorageCategory::ALL
            .iter()
            .map(|c| {
                let i = c.index();
                match c.reservation_mode() {
                    ReservationMode::Envelope => state.reserved[i].saturating_sub(state.scanned[i]),
                    ReservationMode::Additive => state.reserved[i],
                }
            })
            .fold(0u64, u64::saturating_add)
    }

    fn headroom(&self, state: &State) -> u64 {
        let (_, total) = self.charges(state);
        let cap_headroom = self.config.cap_bytes.saturating_sub(total);
        match state.physical_free {
            Some(free) => cap_headroom.min(free.saturating_sub(self.pending(state)).saturating_sub(self.config.physical_margin_bytes)),
            None => cap_headroom,
        }
    }

    /// Reserve `bytes` in `category`, or say which limit refused. Never blocks, never reclaims.
    pub fn reserve(self: &Arc<Self>, category: StorageCategory, bytes: u64) -> Result<StorageReservation, StorageRefusal> {
        let mut state = self.state.lock();
        self.check_locked(&state, category, bytes, true)?;
        state.reserved[category.index()] = state.reserved[category.index()].saturating_add(bytes);
        Ok(StorageReservation { budget: Arc::clone(self), category, bytes })
    }

    fn check_locked(
        &self,
        state: &State,
        category: StorageCategory,
        bytes: u64,
        refuse_when_paused: bool,
    ) -> Result<(), StorageRefusal> {
        let refusal = |reason, available| Err(StorageRefusal { category, requested: bytes, reason, available });
        if refuse_when_paused && state.paused {
            return refusal(StorageRefusalReason::Paused, 0);
        }
        let i = category.index();
        // The charge of the category and of the whole root once this reservation is added.
        let (charge, total) = self.charges(state);
        let new_cat = match category.reservation_mode() {
            ReservationMode::Envelope => state.scanned[i].max(state.reserved[i].saturating_add(bytes)),
            ReservationMode::Additive => charge[i].saturating_add(bytes),
        };
        let added = new_cat.saturating_sub(charge[i]);
        let usable = self.config.cap_bytes.saturating_sub(self.config.safety_margin_bytes);
        if total.saturating_add(added) > usable {
            return refusal(StorageRefusalReason::Cap, usable.saturating_sub(total));
        }
        let budget = self.config.budgets[i];
        // Additive categories with organic growth (the active DB, retained material) may already sit
        // above their budget from scanned bytes alone; their reservations are limited to the
        // reservations' own share of the budget so a growing DB does not block every log rotation.
        let cat_charge_for_budget = match category.reservation_mode() {
            ReservationMode::Envelope => new_cat,
            ReservationMode::Additive => state.reserved[i].saturating_add(bytes),
        };
        if cat_charge_for_budget > budget {
            let used = match category.reservation_mode() {
                ReservationMode::Envelope => charge[i],
                ReservationMode::Additive => state.reserved[i],
            };
            return refusal(StorageRefusalReason::CategoryBudget, budget.saturating_sub(used));
        }
        if let Some(free) = state.physical_free {
            let avail = free.saturating_sub(self.pending(state)).saturating_sub(self.config.physical_margin_bytes);
            if added > avail {
                return refusal(StorageRefusalReason::PhysicalVolume, avail);
            }
        }
        Ok(())
    }

    /// **Reclaim before use**: run the obsolete-data reclaimers, rescan, reserve; if still refused for
    /// lack of space, reclaim under pressure (never liability material), rescan and try once more.
    pub fn reserve_after_reclaim(
        self: &Arc<Self>,
        category: StorageCategory,
        bytes: u64,
    ) -> Result<StorageReservation, StorageRefusal> {
        self.run_reclaimers(ReclaimMode::Obsolete);
        self.refresh_measurement();
        match self.reserve(category, bytes) {
            Ok(r) => Ok(r),
            Err(refusal) if matches!(refusal.reason, StorageRefusalReason::Cap | StorageRefusalReason::PhysicalVolume) => {
                let need = bytes.saturating_sub(refusal.available);
                self.run_reclaimers(ReclaimMode::UnderPressure { need });
                self.refresh_measurement();
                self.reserve(category, bytes)
            }
            Err(refusal) => Err(refusal),
        }
    }

    /// Run every reclaimer once; the bytes they report freed.
    pub fn run_reclaimers(&self, mode: ReclaimMode) -> u64 {
        let reclaimers: Vec<_> = self.reclaimers.lock().clone();
        let mut freed = 0u64;
        for r in reclaimers {
            if let ReclaimMode::UnderPressure { need } = mode
                && freed >= need
            {
                break;
            }
            let mode = match mode {
                ReclaimMode::UnderPressure { need } => ReclaimMode::UnderPressure { need: need.saturating_sub(freed) },
                m => m,
            };
            let got = r.reclaim(mode);
            if got > 0 {
                log::info!("[storage] reclaimer {} freed {} bytes ({:?})", r.name(), got, mode);
            }
            freed = freed.saturating_add(got);
        }
        freed
    }

    /// Scan and probe, without changing the pause state.
    pub fn refresh_measurement(&self) {
        let scanned = self.scan();
        let free = (self.probe)(&self.config.root);
        let mut state = self.state.lock();
        state.scanned = scanned;
        state.physical_free = free;
    }

    /// **One monitor tick**: measure, and enter or leave the low-space pause. Before pausing it
    /// reclaims under pressure; pause hooks run (outside the lock) on entry. Returns the snapshot.
    pub fn refresh(&self) -> StorageSnapshot {
        self.refresh_measurement();
        let short = {
            let state = self.state.lock();
            let headroom = self.headroom(&state);
            // Below the margin — or still paused and not yet back above the resume margin — reclaim
            // first: data may have become reclaimable since the last tick.
            (headroom < self.config.safety_margin_bytes || (state.paused && headroom < self.config.resume_margin_bytes))
                .then(|| self.config.resume_margin_bytes - headroom.min(self.config.resume_margin_bytes))
        };
        if let Some(need) = short {
            self.run_reclaimers(ReclaimMode::Obsolete);
            self.run_reclaimers(ReclaimMode::UnderPressure { need });
            self.refresh_measurement();
        }
        let (entered, left) = {
            let mut state = self.state.lock();
            let headroom = self.headroom(&state);
            let mut entered = false;
            let mut left = false;
            if !state.paused && headroom < self.config.safety_margin_bytes {
                state.paused = true;
                state.paused_since = Some(Instant::now());
                self.paused_flag.store(true, Ordering::Release);
                entered = true;
            } else if state.paused && headroom >= self.config.resume_margin_bytes {
                state.paused = false;
                state.paused_since = None;
                self.paused_flag.store(false, Ordering::Release);
                left = true;
            }
            (entered, left)
        };
        if entered {
            log::warn!(
                "[storage] headroom under the safety margin of {} bytes: PAUSING write intake (IBD, block relay, block submission). \
                 State is flushed; nothing is invalidated. The node resumes once {} bytes are free under the cap.",
                self.config.safety_margin_bytes,
                self.config.resume_margin_bytes
            );
            for hook in self.pause_hooks.lock().iter() {
                hook();
            }
        }
        if left {
            log::info!("[storage] headroom recovered: resuming write intake");
            self.resumed.notify(usize::MAX);
        }
        self.snapshot()
    }

    /// How long the node has been in the low-space pause.
    pub fn paused_for(&self) -> Option<Duration> {
        self.state.lock().paused_since.map(|t| t.elapsed())
    }

    pub fn snapshot(&self) -> StorageSnapshot {
        let state = self.state.lock();
        let (charge, charge_total) = self.charges(&state);
        StorageSnapshot {
            scanned: state.scanned,
            reserved: state.reserved,
            charge,
            charge_total,
            cap_bytes: self.config.cap_bytes,
            physical_free: state.physical_free,
            headroom: self.headroom(&state),
            paused: state.paused,
        }
    }

    fn release(&self, category: StorageCategory, bytes: u64) {
        let mut state = self.state.lock();
        let i = category.index();
        state.reserved[i] = state.reserved[i].saturating_sub(bytes);
    }

    fn try_grow(&self, category: StorageCategory, extra: u64) -> Result<(), StorageRefusal> {
        let mut state = self.state.lock();
        // Growing an existing reservation is not refused by the pause itself — the write it covers is
        // already in progress; finishing it cleanly is what releases space — but it never crosses
        // a limit.
        self.check_locked(&state, category, extra, false)?;
        state.reserved[category.index()] = state.reserved[category.index()].saturating_add(extra);
        Ok(())
    }

    /// Allocated bytes per category, from a walk of the root and every registered path.
    fn scan(&self) -> [u64; STORAGE_CATEGORY_COUNT] {
        let paths = self.paths.lock().clone();
        let mut out = [0u64; STORAGE_CATEGORY_COUNT];
        let root = &self.config.root;
        // Walk the root, plus every registered path that is not under it (e.g. an external WAL dir).
        let mut tops: Vec<PathBuf> = vec![root.clone()];
        for (p, _) in &paths {
            if !p.starts_with(root) && !tops.iter().any(|t| p.starts_with(t)) {
                tops.push(p.clone());
            }
        }
        for top in tops {
            walk(&top, 0, &mut |file, bytes| {
                let category = classify(&paths, file);
                out[category.index()] = out[category.index()].saturating_add(bytes);
            });
        }
        out
    }
}

/// The category of a file: the longest assigned prefix; RocksDB WAL and info-log files of the
/// active DBs are maintenance overhead.
fn classify(paths: &[(PathBuf, StorageCategory)], file: &Path) -> StorageCategory {
    let category = paths.iter().find(|(p, _)| file.starts_with(p)).map(|(_, c)| *c).unwrap_or(StorageCategory::Other);
    if category == StorageCategory::ActiveDb
        && let Some(name) = file.file_name().and_then(|n| n.to_str())
        && (name.ends_with(".log") || name == "LOG" || name.starts_with("LOG.old"))
    {
        return StorageCategory::WalMaintenance;
    }
    category
}

const MAX_WALK_DEPTH: usize = 16;

fn walk(dir: &Path, depth: usize, f: &mut impl FnMut(&Path, u64)) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        // Never follow a symlink: the managed root is what is measured.
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.is_dir() {
            walk(&path, depth + 1, f);
        } else if meta.is_file() {
            f(&path, allocated_bytes(&meta));
        }
    }
}

/// The bytes a file occupies on disk (allocated blocks on unix, so preallocated WAL tails count).
pub fn allocated_bytes(meta: &std::fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (meta.blocks() * 512).max(if meta.blocks() == 0 { meta.len() } else { 0 })
    }
    #[cfg(not(unix))]
    {
        meta.len()
    }
}

/// Allocated bytes of everything under `path` (a file or a directory).
pub fn disk_usage(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if meta.is_file() {
        return allocated_bytes(&meta);
    }
    let mut total = 0u64;
    walk(path, 0, &mut |_, b| total = total.saturating_add(b));
    total
}

/// Free bytes of the volume holding `path` — the deepest mount point that contains it.
pub fn volume_free_bytes(path: &Path) -> Option<u64> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut probe = path.to_path_buf();
        while !probe.exists() {
            match probe.parent() {
                Some(parent) if parent != probe => probe = parent.to_path_buf(),
                _ => break,
            }
        }
        let probe = probe.canonicalize().unwrap_or(probe);
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let mut best: Option<(usize, u64)> = None;
        for disk in disks.list() {
            let mount = disk.mount_point();
            if probe.starts_with(mount) {
                let len = mount.as_os_str().len();
                if best.is_none_or(|(l, _)| len > l) {
                    best = Some((len, disk.available_space()));
                }
            }
        }
        best.map(|(_, free)| free)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        None
    }
}

/// A held reservation; released on drop.
pub struct StorageReservation {
    budget: Arc<StorageBudget>,
    category: StorageCategory,
    bytes: u64,
}

impl StorageReservation {
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn category(&self) -> StorageCategory {
        self.category
    }

    /// Make sure the reservation covers at least `bytes` (a stream whose total is known only once
    /// it starts). Refused growth leaves the reservation as it was.
    pub fn ensure(&mut self, bytes: u64) -> Result<(), StorageRefusal> {
        if bytes <= self.bytes {
            return Ok(());
        }
        let extra = bytes - self.bytes;
        self.budget.try_grow(self.category, extra)?;
        self.bytes = bytes;
        Ok(())
    }
}

impl Drop for StorageReservation {
    fn drop(&mut self) {
        self.budget.release(self.category, self.bytes);
    }
}

impl fmt::Debug for StorageReservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StorageReservation").field("category", &self.category).field("bytes", &self.bytes).finish()
    }
}

static GLOBAL: OnceLock<Arc<StorageBudget>> = OnceLock::new();

/// Install the process's budget (the daemon does this once when `--storage-cap-bytes` is set).
/// A second install is refused and returns the argument back.
pub fn install_global(budget: Arc<StorageBudget>) -> Result<(), Arc<StorageBudget>> {
    GLOBAL.set(budget)
}

/// The process's budget, if the node runs with one.
pub fn global() -> Option<&'static Arc<StorageBudget>> {
    GLOBAL.get()
}

/// Reserve through the process's budget for a small, frequent write: the reservation is taken at
/// once when it fits, and only a refusal for lack of space runs the reclaimers (one scan) and tries
/// again. `Ok(None)` when no budget is installed (unchanged behaviour).
pub fn reserve_global(category: StorageCategory, bytes: u64) -> Result<Option<StorageReservation>, StorageRefusal> {
    match global() {
        Some(b) => match b.reserve(category, bytes) {
            Ok(r) => Ok(Some(r)),
            Err(refusal) if matches!(refusal.reason, StorageRefusalReason::Cap | StorageRefusalReason::PhysicalVolume) => {
                b.reserve_after_reclaim(category, bytes).map(Some)
            }
            Err(refusal) => Err(refusal),
        },
        None => Ok(None),
    }
}

/// Reserve through the process's budget for a LARGE operation (an IBD spill, a staging DB): obsolete
/// data is always reclaimed first, so new temp space is never taken while reclaimable history sits
/// on disk. `Ok(None)` when no budget is installed.
pub fn reserve_global_after_reclaim(category: StorageCategory, bytes: u64) -> Result<Option<StorageReservation>, StorageRefusal> {
    match global() {
        Some(b) => b.reserve_after_reclaim(category, bytes).map(Some),
        None => Ok(None),
    }
}

/// `true` unless an installed budget is in the low-space pause.
pub fn global_writable() -> bool {
    global().is_none_or(|b| !b.is_paused())
}

/// Wait for an installed budget to leave the low-space pause (immediate without a budget).
pub async fn wait_global_writable() {
    if let Some(b) = global() {
        b.wait_until_writable().await;
    }
}

/// The directory managed temp / spill files go to: `<root>/storage-tmp` when a budget is installed,
/// otherwise the OS temp dir (unchanged behaviour).
pub fn managed_temp_dir() -> PathBuf {
    match global() {
        Some(b) => {
            let dir = b.root().join(STORAGE_TMP_DIR);
            let _ = std::fs::create_dir_all(&dir);
            dir
        }
        None => std::env::temp_dir(),
    }
}

/// The managed temp directory's name under the root.
pub const STORAGE_TMP_DIR: &str = "storage-tmp";

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    const MB: u64 = 1_000_000;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("storage-budget-{tag}-{}-{}", std::process::id(), rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A volume of `size` bytes that the test fills by writing files under `root`.
    fn fake_volume(root: PathBuf, size: u64) -> FreeSpaceProbe {
        Arc::new(move |_| Some(size.saturating_sub(disk_usage(&root))))
    }

    fn write_file(path: &Path, bytes: u64) {
        use std::io::Write;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(path).unwrap();
        let chunk = vec![0xA5u8; 64 * 1024];
        let mut left = bytes;
        while left > 0 {
            let n = left.min(chunk.len() as u64) as usize;
            f.write_all(&chunk[..n]).unwrap();
            left -= n as u64;
        }
        f.sync_all().unwrap();
    }

    /// A 50 MB cap with the design shape: active 17.5, staging 17.5, wal 6.67, retained 3.33,
    /// spill 1.34, logs 3.33, other 0.33 (MB), margin 1 MB, resume 2 MB.
    fn budget_50mb(root: &Path, volume: u64) -> Arc<StorageBudget> {
        let mut config = StorageBudgetConfig::for_cap(root.to_path_buf(), 50 * MB);
        config.safety_margin_bytes = MB;
        config.resume_margin_bytes = 2 * MB;
        config.physical_margin_bytes = 0;
        let b = StorageBudget::with_probe(config, fake_volume(root.to_path_buf(), volume));
        b.assign_path(root.join("datadir/consensus"), StorageCategory::ActiveDb);
        b.assign_path(root.join("palw-retention"), StorageCategory::PalwRetained);
        b.assign_path(root.join("logs"), StorageCategory::Logs);
        b.assign_path(root.join(STORAGE_TMP_DIR), StorageCategory::IbdSpill);
        b
    }

    #[test]
    fn the_design_budget_sums_to_the_recommended_cap_and_scales_down() {
        assert_eq!(DESIGN_BUDGET_30GB.iter().sum::<u64>(), RECOMMENDED_STORAGE_CAP_BYTES);
        let small = budgets_for_cap(50 * MB);
        assert!(small.iter().sum::<u64>() <= 50 * MB);
        assert_eq!(small[StorageCategory::ActiveDb.index()], small[StorageCategory::StagingDb.index()]);
        assert!(StorageBudgetConfig::for_cap(PathBuf::from("/x"), 50 * MB).warnings().is_empty());
    }

    /// Deliverable 5a: two concurrent tasks cannot both count the same free bytes.
    #[test]
    fn reservations_prevent_double_use_of_the_same_free_bytes() {
        let root = tempdir("double");
        let b = budget_50mb(&root, 1 << 40);
        b.refresh();
        // Usable = 50 − 1 (margin) = 49 MB; two 30 MB additive reservations cannot both fit even
        // though each alone would.
        let first = b.reserve(StorageCategory::WalMaintenance, 6 * MB).unwrap();
        let mut held = vec![first];
        // Fill the cap with additive "other" reservations up to the category limits: logs 3.33 MB.
        held.push(b.reserve(StorageCategory::Logs, 3 * MB).unwrap());
        let refused = b.reserve(StorageCategory::Logs, MB).unwrap_err();
        assert_eq!(refused.reason, StorageRefusalReason::CategoryBudget, "{refused}");
        // Envelope categories: one IBD spill session of 1.2 MB fits the 1.34 MB budget, a second does not.
        let spill = b.reserve(StorageCategory::IbdSpill, 1_200_000).unwrap();
        assert_eq!(b.reserve(StorageCategory::IbdSpill, 1_200_000).unwrap_err().reason, StorageRefusalReason::CategoryBudget);
        drop(spill);
        let spill_again = b.reserve(StorageCategory::IbdSpill, 1_200_000).unwrap();
        // Cap: active DB bytes on disk + staging reservation must stay under 49 MB.
        write_file(&root.join("datadir/consensus/consensus-001/000001.sst"), 15 * MB);
        b.refresh();
        let staging = b.reserve(StorageCategory::StagingDb, 17 * MB).unwrap();
        let snap = b.snapshot();
        assert!(snap.charge_total <= 49 * MB, "{}", snap.summary_line());
        // Everything else left: 49 − (15 + 17 + 6 + 3 + 1.2) ≈ 6.8 MB; a 7 MB request is refused by the cap.
        let err = b.reserve(StorageCategory::PalwRetained, 7 * MB).unwrap_err();
        assert_eq!(err.reason, StorageRefusalReason::Cap, "{err}");
        assert!(b.reserve(StorageCategory::PalwRetained, 3 * MB).is_ok());
        drop((held, spill_again, staging));
        assert_eq!(b.snapshot().reserved, [0; STORAGE_CATEGORY_COUNT], "every guard released its bytes");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Deliverable 5b: an IBD-like staging DB grows next to the active DB inside its reservation and
    /// the pair never exceeds the cap; a staging DB that would not fit is refused BEFORE it is created.
    #[test]
    fn ibd_like_staging_next_to_the_active_db_stays_within_the_cap() {
        let root = tempdir("staging");
        let b = budget_50mb(&root, 1 << 40);
        let consensus = root.join("datadir/consensus");
        write_file(&consensus.join("consensus-001/000001.sst"), 12 * MB);
        b.refresh();
        // The factory's sequence: reserve, then assign the new dir to the staging category.
        let staging = b.reserve_after_reclaim(StorageCategory::StagingDb, 17 * MB).unwrap();
        b.assign_path(consensus.join("consensus-002"), StorageCategory::StagingDb);
        for i in 0..8 {
            write_file(&consensus.join(format!("consensus-002/{i:06}.sst")), 2 * MB);
            let snap = b.refresh();
            // The envelope does not double-count what the staging DB already wrote.
            assert_eq!(snap.charge[StorageCategory::StagingDb.index()], 17 * MB, "{}", snap.summary_line());
            assert!(snap.charge_total + b.config().safety_margin_bytes <= b.config().cap_bytes, "{}", snap.summary_line());
            assert!(!snap.paused);
        }
        // The commit: staging becomes active, the old active dir is deleted, the reservation released.
        std::fs::remove_dir_all(consensus.join("consensus-001")).unwrap();
        b.unassign_path(&consensus.join("consensus-002"));
        drop(staging);
        let snap = b.refresh();
        assert_eq!(snap.charge[StorageCategory::StagingDb.index()], 0);
        assert!(snap.charge[StorageCategory::ActiveDb.index()] >= 16 * MB);
        // A second staging DB the size of the budget does not fit beside a 16 MB active DB plus a
        // 20 MB retained pile: refused up front, nothing created.
        write_file(&root.join("palw-retention/owed.material"), 20 * MB);
        b.refresh();
        let err = b.reserve_after_reclaim(StorageCategory::StagingDb, 17 * MB).unwrap_err();
        assert_eq!(err.reason, StorageRefusalReason::Cap, "{err}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Deliverable 5c: the low-space pause engages under the margin, hooks run, reservations are
    /// refused, and the node resumes (waking async waiters) once space is reclaimed.
    #[test]
    fn low_space_pauses_intake_then_resumes_when_space_is_reclaimed() {
        let root = tempdir("pause");
        let b = budget_50mb(&root, 1 << 40);
        let flushed = Arc::new(AtomicU64::new(0));
        let f = Arc::clone(&flushed);
        b.add_pause_hook(move || {
            f.fetch_add(1, Ordering::SeqCst);
        });
        write_file(&root.join("datadir/consensus/consensus-001/a.sst"), 30 * MB);
        assert!(!b.refresh().paused);
        // Organic growth of live state (not a reservation) pushes headroom under the 1 MB margin.
        write_file(&root.join("datadir/consensus/consensus-001/b.sst"), 19_500_000);
        let snap = b.refresh();
        assert!(snap.paused, "{}", snap.summary_line());
        assert!(b.is_paused() && !global_writable_for(&b));
        assert_eq!(flushed.load(Ordering::SeqCst), 1, "pause hooks (WAL fsync) ran once on entry");
        assert_eq!(b.reserve(StorageCategory::Logs, 1000).unwrap_err().reason, StorageRefusalReason::Paused);
        // Still paused at 1.5 MB headroom (hysteresis: resume needs 2 MB).
        std::fs::remove_file(root.join("datadir/consensus/consensus-001/b.sst")).unwrap();
        write_file(&root.join("datadir/consensus/consensus-001/b.sst"), 18_500_000);
        assert!(b.refresh().paused);
        assert_eq!(flushed.load(Ordering::SeqCst), 1, "hooks do not re-run while already paused");
        // A waiter blocked on the pause is released by the resume.
        let waiter = {
            let b = Arc::clone(&b);
            std::thread::spawn(move || b.wait_until_writable_blocking(Duration::from_secs(20)))
        };
        std::thread::sleep(Duration::from_millis(100));
        std::fs::remove_file(root.join("datadir/consensus/consensus-001/b.sst")).unwrap();
        let snap = b.refresh();
        assert!(!snap.paused, "{}", snap.summary_line());
        assert!(waiter.join().unwrap(), "the blocked intake was woken by the resume");
        assert!(b.reserve(StorageCategory::Logs, 1000).is_ok());
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn global_writable_for(b: &StorageBudget) -> bool {
        !b.is_paused()
    }

    /// The physical volume is a limit too: a cap larger than the disk does not let reservations
    /// overrun the disk, and pending (not yet written) reservations are subtracted from free space.
    #[test]
    fn the_physical_volume_limits_reservations_below_the_cap() {
        let root = tempdir("phys");
        // A 20 MB "disk" under a 50 MB cap.
        let b = budget_50mb(&root, 20 * MB);
        b.refresh();
        let spill = b.reserve(StorageCategory::IbdSpill, 1_300_000).unwrap();
        let staging = b.reserve(StorageCategory::StagingDb, 17 * MB).unwrap();
        // 20 − 1.3 − 17 = 1.7 MB physically left for new bytes.
        let err = b.reserve(StorageCategory::WalMaintenance, 2 * MB).unwrap_err();
        assert_eq!(err.reason, StorageRefusalReason::PhysicalVolume, "{err}");
        drop((spill, staging));
        std::fs::remove_dir_all(&root).unwrap();
    }

    struct DirReclaimer {
        dir: PathBuf,
        calls: Mutex<Vec<ReclaimMode>>,
        log: Arc<Mutex<Vec<String>>>,
    }
    impl StorageReclaimer for DirReclaimer {
        fn name(&self) -> &str {
            "test-obsolete"
        }
        fn reclaim(&self, mode: ReclaimMode) -> u64 {
            self.calls.lock().push(mode);
            self.log.lock().push(format!("reclaim {mode:?}"));
            if mode != ReclaimMode::Obsolete {
                return 0;
            }
            let freed = disk_usage(&self.dir);
            let _ = std::fs::remove_dir_all(&self.dir);
            freed
        }
    }

    /// Deliverable 5d: obsolete data (an orphaned staging dir) is reclaimed BEFORE the new space is
    /// reserved, so a reservation that only fits after the GC succeeds.
    #[test]
    fn gc_frees_obsolete_data_before_the_reservation_is_taken() {
        let root = tempdir("gc");
        let b = budget_50mb(&root, 1 << 40);
        let orphan = root.join("datadir/consensus/consensus-007");
        write_file(&orphan.join("000001.sst"), 20 * MB);
        write_file(&root.join("datadir/consensus/consensus-008/000001.sst"), 15 * MB);
        b.refresh();
        // Without reclaim the staging reservation does not fit (35 + 17 > 49).
        assert_eq!(b.reserve(StorageCategory::StagingDb, 17 * MB).unwrap_err().reason, StorageRefusalReason::Cap);
        let log = Arc::new(Mutex::new(Vec::new()));
        let r = Arc::new(DirReclaimer { dir: orphan.clone(), calls: Mutex::new(Vec::new()), log: Arc::clone(&log) });
        b.register_reclaimer(r.clone());
        let staging = b.reserve_after_reclaim(StorageCategory::StagingDb, 17 * MB).unwrap();
        assert!(!orphan.exists(), "the orphan went first");
        assert_eq!(r.calls.lock().first(), Some(&ReclaimMode::Obsolete), "obsolete reclaim runs before any reservation");
        assert_eq!(staging.bytes(), 17 * MB);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A reclaimer that models the retention janitor's contract: it frees only unowed material and
    /// reports what it freed; owed material stays even when the reservation must then be refused.
    struct LiabilityAwareReclaimer {
        dir: PathBuf,
    }
    impl StorageReclaimer for LiabilityAwareReclaimer {
        fn name(&self) -> &str {
            "test-retention"
        }
        fn reclaim(&self, mode: ReclaimMode) -> u64 {
            let ReclaimMode::UnderPressure { mut need } = mode else { return 0 };
            let mut freed = 0;
            for entry in std::fs::read_dir(&self.dir).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if need == 0 || name.starts_with("owed-") {
                    continue;
                }
                let bytes = disk_usage(&entry.path());
                std::fs::remove_file(entry.path()).unwrap();
                freed += bytes;
                need = need.saturating_sub(bytes);
            }
            freed
        }
    }

    /// Deliverable 5e: retained liability material is never deleted to fit; the reservation is refused
    /// instead (and the low-space pause, not deletion, is what a node in that state gets).
    #[test]
    fn retained_liability_material_is_never_deleted_to_fit() {
        let root = tempdir("liability");
        let b = budget_50mb(&root, 1 << 40);
        let retention = root.join("palw-retention");
        write_file(&retention.join("owed-1.material"), 17 * MB);
        write_file(&retention.join("owed-2.material"), 17 * MB);
        write_file(&retention.join("done-3.material"), 5 * MB);
        b.register_reclaimer(Arc::new(LiabilityAwareReclaimer { dir: retention.clone() }));
        b.refresh();
        // 39 MB retained + 17 MB staging > 49: the unowed 5 MB goes, the owed 34 MB stays, refused.
        let err = b.reserve_after_reclaim(StorageCategory::StagingDb, 17 * MB).unwrap_err();
        assert_eq!(err.reason, StorageRefusalReason::Cap, "{err}");
        assert!(!retention.join("done-3.material").exists());
        assert!(retention.join("owed-1.material").exists() && retention.join("owed-2.material").exists());
        // Organic growth of owed material into the margin pauses the node rather than deleting it.
        write_file(&retention.join("owed-4.material"), 19 * MB);
        assert!(b.refresh().paused);
        for n in ["owed-1.material", "owed-2.material", "owed-4.material"] {
            assert!(retention.join(n).exists(), "{n} survived the pause");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn log_reclaim_drops_archives_beyond_the_window_first_and_never_the_active_log() {
        let dir = tempdir("logs");
        write_file(&dir.join("rusty-kaspa.log"), 300_000);
        for n in 1..=10u32 {
            write_file(&dir.join(format!("rusty-kaspa.log.{n}.gz")), 100_000);
        }
        write_file(&dir.join("rusty-kaspa_err.log.3.gz"), 100_000);
        assert!(reclaim_log_archives(&dir, 8, ReclaimMode::Obsolete) >= 200_000);
        assert!(!dir.join("rusty-kaspa.log.9.gz").exists() && !dir.join("rusty-kaspa.log.10.gz").exists());
        assert!(dir.join("rusty-kaspa.log.8.gz").exists(), "inside the window stays in Obsolete mode");
        reclaim_log_archives(&dir, 8, ReclaimMode::UnderPressure { need: 150_000 });
        assert!(!dir.join("rusty-kaspa.log.8.gz").exists() && !dir.join("rusty-kaspa.log.7.gz").exists(), "oldest first");
        assert!(dir.join("rusty-kaspa.log.1.gz").exists());
        assert!(dir.join("rusty-kaspa.log").exists(), "the active log is never reclaimed");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rocksdb_wal_and_info_logs_of_the_active_db_are_maintenance_and_growth_is_bounded_by_ensure() {
        let root = tempdir("classify");
        let b = budget_50mb(&root, 1 << 40);
        write_file(&root.join("datadir/consensus/consensus-001/000009.log"), 2 * MB);
        write_file(&root.join("datadir/consensus/consensus-001/LOG.old.123"), MB);
        write_file(&root.join("datadir/consensus/consensus-001/000010.sst"), 3 * MB);
        let snap = b.refresh();
        assert!(snap.scanned[StorageCategory::WalMaintenance.index()] >= 3 * MB);
        assert!(snap.scanned[StorageCategory::ActiveDb.index()] >= 3 * MB && snap.scanned[StorageCategory::ActiveDb.index()] < 4 * MB);
        // A spill whose total is only known once the stream starts grows its reservation, within budget.
        let mut spill = b.reserve(StorageCategory::IbdSpill, 600_000).unwrap();
        spill.ensure(1_300_000).unwrap();
        assert_eq!(spill.bytes(), 1_300_000);
        assert_eq!(spill.ensure(2 * MB).unwrap_err().reason, StorageRefusalReason::CategoryBudget);
        assert_eq!(spill.bytes(), 1_300_000, "refused growth leaves the reservation as it was");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
