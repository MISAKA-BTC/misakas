//! Runtime disk guard.
//!
//! `--min-disk-free-percent` was a *preflight*: it refused to start below a
//! threshold and then never looked again. A node that started with 40% free and
//! filled the mount while running got no warning, no rate estimate, and no
//! shutdown — it kept writing until RocksDB hit ENOSPC.
//!
//! This service samples the data mount on a tick and escalates through three
//! levels, all derived from the operator's own `--min-disk-free-percent`:
//!
//! | level     | free space            | action                                        |
//! |-----------|-----------------------|-----------------------------------------------|
//! | Warning   | `min + 10` pts        | log free %, growth rate, hours-to-full        |
//! | Critical  | `min`                 | above + optional work stands down (validator) |
//! | Emergency | `min * 2/3`           | above + graceful shutdown, before corruption  |
//!
//! With the `misaka setup` default of 15 that is 25 / 15 / 10 percent.
//!
//! Two things it deliberately does NOT do. It never deletes anything: a guard
//! that reclaims space on its own is a guard that can delete a database on a
//! misread, and the operator's recovery options are strictly better than any
//! choice this code could make unattended. And it does not try to outrun the
//! problem by pruning harder — pruning is deferred while consensus is
//! transitional or catching up for correctness reasons, so the answer to "the
//! disk is filling during IBD" is to not generate the data (see
//! `--evm-storage-profile=compact`), not to race the pruner.
//!
//! Reporting free space as a percentage rather than bytes is deliberate: the
//! absolute headroom that matters scales with the database, and a percentage is
//! what the operator already configured.

use kaspa_core::{
    core::Core,
    info,
    signals::Shutdown,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace, warn,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Weak,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

pub const SERVICE_NAME: &str = "disk-guard";

/// How often the mount is sampled. A minute is far below the time it takes any
/// realistic growth rate to cross a whole threshold band, and the sample itself
/// is a `statvfs` plus one directory listing.
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(60);

/// How much free space, in percentage points, the Warning band sits above Critical.
const WARNING_MARGIN_POINTS: f64 = 10.0;

/// Growth rates below this are treated as noise rather than projected forward —
/// otherwise a few bytes of drift yields an "exhausted in 900 years" line that
/// only makes the real ones harder to spot.
const MIN_MEANINGFUL_GROWTH_BYTES_PER_HOUR: f64 = 16.0 * 1024.0 * 1024.0;

/// Disk-pressure level, ordered so consumers can compare (`>= Critical`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum DiskPressure {
    Normal = 0,
    Warning = 1,
    Critical = 2,
    Emergency = 3,
}

impl DiskPressure {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Warning,
            2 => Self::Critical,
            3 => Self::Emergency,
            _ => Self::Normal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Warning => "warning",
            Self::Critical => "critical",
            Self::Emergency => "emergency",
        }
    }
}

/// A cheap shared read of the current pressure level.
///
/// Handed to subsystems that produce optional work, so they can stand down
/// before the mount fills rather than after. Cloneable and lock-free; a stale
/// read is harmless because the guard re-evaluates every tick.
#[derive(Clone, Debug, Default)]
pub struct DiskPressureHandle(Arc<AtomicU8>);

impl DiskPressureHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn level(&self) -> DiskPressure {
        DiskPressure::from_u8(self.0.load(Ordering::Relaxed))
    }

    fn set(&self, level: DiskPressure) {
        self.0.store(level as u8, Ordering::Relaxed);
    }

    /// Whether work that is safe to skip should stand down. True from Critical up.
    ///
    /// "Safe to skip" means the node stays correct without it — attestations,
    /// mining templates. Never block consensus validation on this: falling behind
    /// the network is not an improvement over a full disk, and a node that stops
    /// validating still has to catch up later on the same disk.
    pub fn should_pause_optional_work(&self) -> bool {
        self.level() >= DiskPressure::Critical
    }
}

/// The three bands, derived from `--min-disk-free-percent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiskGuardThresholds {
    pub warning_percent: f64,
    pub critical_percent: f64,
    pub emergency_percent: f64,
}

impl DiskGuardThresholds {
    /// `None` when `min_disk_free_percent` is 0 — the operator turned the disk
    /// check off, and the runtime guard honours that just as the preflight does.
    pub fn from_min_free_percent(min_disk_free_percent: u8) -> Option<Self> {
        if min_disk_free_percent == 0 {
            return None;
        }
        let critical = min_disk_free_percent as f64;
        Some(Self {
            warning_percent: critical + WARNING_MARGIN_POINTS,
            critical_percent: critical,
            // Two thirds of the operator's own floor: far enough below Critical that
            // the levels are distinguishable, high enough that a clean RocksDB close
            // still has room to write.
            emergency_percent: critical * 2.0 / 3.0,
        })
    }

    pub fn classify(&self, free_percent: f64) -> DiskPressure {
        if free_percent < self.emergency_percent {
            DiskPressure::Emergency
        } else if free_percent < self.critical_percent {
            DiskPressure::Critical
        } else if free_percent < self.warning_percent {
            DiskPressure::Warning
        } else {
            DiskPressure::Normal
        }
    }
}

/// Free and total bytes of the mount that `path` lives on.
///
/// Walks up to the nearest existing ancestor, so it works before the data
/// directory has been created, and picks the LONGEST matching mount point so a
/// dedicated `/var/lib/misaka` mount is not mistaken for `/`.
pub fn data_mount_usage(path: &Path) -> Option<(u64, u64)> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        match probe.parent() {
            Some(parent) if parent != probe => probe = parent.to_path_buf(),
            _ => break,
        }
    }
    let probe = probe.canonicalize().unwrap_or(probe);

    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut best: Option<(usize, u64, u64)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point();
        if probe.starts_with(mount) {
            let len = mount.as_os_str().len();
            if best.map(|(best_len, _, _)| len > best_len).unwrap_or(true) {
                best = Some((len, disk.available_space(), disk.total_space()));
            }
        }
    }
    best.map(|(_, available, total)| (available, total))
}

pub fn data_mount_free_percent(path: &Path) -> Option<f64> {
    data_mount_usage(path).and_then(|(available, total)| (total > 0).then(|| available as f64 / total as f64 * 100.0))
}

/// Recursive size of a directory in bytes, ignoring entries that vanish mid-walk
/// (RocksDB compaction deletes SSTs underneath us constantly).
pub fn dir_size_bytes(path: &Path) -> Option<u64> {
    fn walk(path: &Path, total: &mut u64, depth: usize) {
        // RocksDB lays its files out shallowly; the bound just stops a symlink
        // loop from turning a monitoring tick into an unbounded walk.
        if depth > 8 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else { return };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                walk(&entry.path(), total, depth + 1);
            } else {
                *total += meta.len();
            }
        }
    }
    if !path.exists() {
        return None;
    }
    let mut total = 0;
    walk(path, &mut total, 0);
    Some(total)
}

/// Hours until the mount is full at the observed growth rate. `None` when growth
/// is nil, negative (reclamation is winning) or below the noise floor.
pub fn hours_to_full(free_bytes: u64, growth_bytes_per_hour: f64) -> Option<f64> {
    (growth_bytes_per_hour >= MIN_MEANINGFUL_GROWTH_BYTES_PER_HOUR).then(|| free_bytes as f64 / growth_bytes_per_hour)
}

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

pub struct DiskGuard {
    tick_service: Arc<TickService>,
    app_dir: PathBuf,
    /// Watched separately from the mount so the log says WHICH store is growing.
    consensus_db_dir: PathBuf,
    thresholds: DiskGuardThresholds,
    interval: Duration,
    pressure: DiskPressureHandle,
    /// Weak so the guard — which is itself owned by the core's service list —
    /// does not keep the core alive.
    core: Weak<Core>,
}

impl DiskGuard {
    pub fn new(
        tick_service: Arc<TickService>,
        app_dir: PathBuf,
        consensus_db_dir: PathBuf,
        thresholds: DiskGuardThresholds,
        pressure: DiskPressureHandle,
        core: &Arc<Core>,
    ) -> Self {
        Self { tick_service, app_dir, consensus_db_dir, thresholds, interval: SAMPLE_INTERVAL, pressure, core: Arc::downgrade(core) }
    }

    pub async fn worker(&self) {
        info!(
            "[{SERVICE_NAME}] watching {} — warning < {:.0}% free, critical < {:.0}%, emergency < {:.0}% (graceful shutdown)",
            self.app_dir.display(),
            self.thresholds.warning_percent,
            self.thresholds.critical_percent,
            self.thresholds.emergency_percent,
        );

        let mut last_sample: Option<(Instant, u64)> = None;
        let mut last_level = DiskPressure::Normal;

        while let TickReason::Wakeup = self.tick_service.tick(self.interval).await {
            let Some((free_bytes, total_bytes)) = data_mount_usage(&self.app_dir) else {
                trace!("[{SERVICE_NAME}] could not read free space for {}; skipping this sample", self.app_dir.display());
                continue;
            };
            if total_bytes == 0 {
                continue;
            }
            let free_percent = free_bytes as f64 / total_bytes as f64 * 100.0;
            let consensus_bytes = dir_size_bytes(&self.consensus_db_dir);

            // Growth of the consensus DB itself, not of the mount: other tenants on
            // the mount are the operator's problem, but a runaway consensus store is
            // ours, and attributing it correctly is the whole point of the report
            // this guard came from.
            let now = Instant::now();
            let growth_per_hour = match (last_sample, consensus_bytes) {
                (Some((prev_at, prev_bytes)), Some(bytes)) => {
                    let elapsed_hours = now.duration_since(prev_at).as_secs_f64() / 3600.0;
                    (elapsed_hours > 0.0).then(|| (bytes as f64 - prev_bytes as f64) / elapsed_hours)
                }
                _ => None,
            };
            if let Some(bytes) = consensus_bytes {
                last_sample = Some((now, bytes));
            }

            let level = self.thresholds.classify(free_percent);
            self.pressure.set(level);

            if level == DiskPressure::Normal {
                if last_level != DiskPressure::Normal {
                    info!("[{SERVICE_NAME}] recovered: {free_percent:.1}% free on {}", self.app_dir.display());
                }
                last_level = level;
                continue;
            }

            let consensus_note = consensus_bytes
                .map(|b| format!(", consensus DB {:.1} GiB", b as f64 / GIB))
                .unwrap_or_else(|| ", consensus DB size unknown".to_string());
            let growth_note = growth_per_hour
                .filter(|g| g.abs() >= MIN_MEANINGFUL_GROWTH_BYTES_PER_HOUR)
                .map(|g| format!(" ({:+.2} GiB/h)", g / GIB))
                .unwrap_or_default();
            // What actually decides whether an operator has time to intervene is not
            // the free percentage but how long it lasts at the current rate.
            let eta_note = growth_per_hour
                .and_then(|g| hours_to_full(free_bytes, g))
                .map(|h| format!(", exhausted in ~{h:.1}h at this rate"))
                .unwrap_or_default();

            match level {
                DiskPressure::Warning => warn!(
                    "[{SERVICE_NAME}] disk WARNING: {:.1}% free ({:.1} GiB) on {}{}{}{}",
                    free_percent,
                    free_bytes as f64 / GIB,
                    self.app_dir.display(),
                    consensus_note,
                    growth_note,
                    eta_note,
                ),
                DiskPressure::Critical => warn!(
                    "[{SERVICE_NAME}] disk CRITICAL: {:.1}% free ({:.1} GiB) on {}{}{}{} — optional work (validator \
                     attestation) is standing down. Free space or stop the node; the next band is a graceful shutdown at {:.0}%.",
                    free_percent,
                    free_bytes as f64 / GIB,
                    self.app_dir.display(),
                    consensus_note,
                    growth_note,
                    eta_note,
                    self.thresholds.emergency_percent,
                ),
                DiskPressure::Emergency => {
                    kaspa_core::error!(
                        "[{SERVICE_NAME}] disk EMERGENCY: {:.1}% free ({:.1} GiB) on {}{}{}{} — shutting down GRACEFULLY \
                         now so RocksDB closes cleanly instead of hitting ENOSPC mid-write. NOTHING has been deleted. \
                         Free space before restarting; if the consensus DB is the thing growing, start with \
                         --evm-storage-profile=compact so it stays bounded.",
                        free_percent,
                        free_bytes as f64 / GIB,
                        self.app_dir.display(),
                        consensus_note,
                        growth_note,
                        eta_note,
                    );
                    if let Some(core) = self.core.upgrade() {
                        core.shutdown();
                    }
                    return;
                }
                DiskPressure::Normal => unreachable!("handled above"),
            }
            last_level = level;
        }
        trace!("[{SERVICE_NAME}] worker exiting");
    }
}

impl AsyncService for DiskGuard {
    fn ident(self: Arc<Self>) -> &'static str {
        SERVICE_NAME
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {SERVICE_NAME}");
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{SERVICE_NAME} stopped");
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_min_free_percent_disables_the_guard() {
        // `0` means the operator turned the disk check off; the runtime guard must
        // honour that exactly as the startup preflight does.
        assert!(DiskGuardThresholds::from_min_free_percent(0).is_none());
    }

    #[test]
    fn thresholds_derive_the_documented_bands_from_the_operator_floor() {
        let t = DiskGuardThresholds::from_min_free_percent(15).unwrap();
        assert_eq!(t.warning_percent, 25.0);
        assert_eq!(t.critical_percent, 15.0);
        assert_eq!(t.emergency_percent, 10.0);
    }

    #[test]
    fn classify_covers_every_band_including_the_boundaries() {
        let t = DiskGuardThresholds::from_min_free_percent(15).unwrap();
        assert_eq!(t.classify(80.0), DiskPressure::Normal);
        // A threshold is the bottom of its own band: exactly 25% is still Normal.
        assert_eq!(t.classify(25.0), DiskPressure::Normal);
        assert_eq!(t.classify(24.9), DiskPressure::Warning);
        assert_eq!(t.classify(15.0), DiskPressure::Warning);
        assert_eq!(t.classify(14.9), DiskPressure::Critical);
        assert_eq!(t.classify(10.0), DiskPressure::Critical);
        assert_eq!(t.classify(9.9), DiskPressure::Emergency);
        assert_eq!(t.classify(0.0), DiskPressure::Emergency);
    }

    #[test]
    fn optional_work_pauses_from_critical_up_only() {
        let handle = DiskPressureHandle::new();
        assert_eq!(handle.level(), DiskPressure::Normal);
        assert!(!handle.should_pause_optional_work());

        handle.set(DiskPressure::Warning);
        assert!(!handle.should_pause_optional_work());

        handle.set(DiskPressure::Critical);
        assert!(handle.should_pause_optional_work());

        handle.set(DiskPressure::Emergency);
        assert!(handle.should_pause_optional_work());
    }

    #[test]
    fn exhaustion_estimate_ignores_flat_and_shrinking_stores() {
        // 100 GiB free, growing 10 GiB/h -> 10 hours.
        let free = 100 * 1024 * 1024 * 1024;
        let rate = 10.0 * GIB;
        let hours = hours_to_full(free, rate).unwrap();
        assert!((hours - 10.0).abs() < 1e-6, "{hours}");

        // Reclamation winning, or drift: no projection rather than a nonsense one.
        assert!(hours_to_full(free, 0.0).is_none());
        assert!(hours_to_full(free, -5.0 * GIB).is_none());
        assert!(hours_to_full(free, 1024.0).is_none());
    }

    #[test]
    fn dir_size_sums_nested_files_and_tolerates_a_missing_dir() {
        let dir = std::env::temp_dir().join(format!("misaka-disk-guard-{}", std::process::id()));
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.join("a.sst"), vec![0u8; 1000]).unwrap();
        std::fs::write(nested.join("b.sst"), vec![0u8; 2000]).unwrap();

        assert_eq!(dir_size_bytes(&dir), Some(3000));
        assert_eq!(dir_size_bytes(&dir.join("does-not-exist")), None);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
