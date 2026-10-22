//! **The node's storage budget, wired** (G14 capacity lane; `kaspa_utils::storage_budget` holds the
//! model, `docs/design/palw/storage/capacity.md` the inventory and the plan).
//!
//! Off unless `--storage-cap-bytes` is given. When on, the daemon:
//! 1. builds the budget over the managed root `<appdir>/<network>` before any database opens, assigns
//!    every known path to its category, and installs it as the process budget (so the consensus
//!    factory, the IBD spill and the intake gates find it);
//! 2. opens every RocksDB with footprint bounds (`StorageDbBounds`);
//! 3. registers the reclaimers that run before large reservations and under pressure — obsolete
//!    consensus dirs, abandoned temp files, old log archives, and the PALW retention janitor's
//!    unowed captures (which never include material the chain can still demand);
//! 4. runs [`StorageBudgetService`]: a periodic scan that drives the low-space pause.
use kaspa_core::{
    info,
    task::service::{AsyncService, AsyncServiceFuture},
    trace, warn,
};
use kaspa_utils::storage_budget::{
    FnReclaimer, ReclaimMode, STORAGE_TMP_DIR, StorageBudget, StorageBudgetConfig, StorageCategory, StorageReservation,
    reclaim_log_archives, reclaim_temp_dir,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const STORAGE_BUDGET_SERVICE: &str = "storage-budget";
/// How often the managed root is measured.
const STORAGE_SCAN_PERIOD: Duration = Duration::from_secs(15);
/// How often the summary line is logged while nothing changes.
const STORAGE_SUMMARY_PERIOD: Duration = Duration::from_secs(600);
/// A file in the managed temp dir older than this was abandoned (the spill session TTL is 600 s).
const STORAGE_TMP_HORIZON: Duration = Duration::from_secs(3600);

/// Parse `--storage-category-budget=<category>=<bytes>` entries.
pub fn parse_category_budgets(entries: &[String]) -> Result<Vec<(StorageCategory, u64)>, String> {
    entries
        .iter()
        .map(|e| {
            let (name, bytes) =
                e.split_once('=').ok_or_else(|| format!("--storage-category-budget={e}: expected <category>=<bytes>"))?;
            let category = StorageCategory::from_name(name.trim())
                .ok_or_else(|| format!("--storage-category-budget={e}: unknown category {name:?}"))?;
            let bytes = bytes.trim().replace('_', "").parse::<u64>().map_err(|why| format!("--storage-category-budget={e}: {why}"))?;
            Ok((category, bytes))
        })
        .collect()
}

/// The budget's configuration from the arguments; `None` when no cap is set.
pub fn storage_budget_config(
    cap: Option<u64>,
    safety_margin: Option<u64>,
    overrides: &[String],
    root: PathBuf,
) -> Result<Option<StorageBudgetConfig>, String> {
    let Some(cap) = cap else { return Ok(None) };
    if cap == 0 {
        return Err("--storage-cap-bytes must be positive".into());
    }
    let mut config = StorageBudgetConfig::for_cap(root, cap);
    if let Some(margin) = safety_margin {
        config.safety_margin_bytes = margin;
        config.physical_margin_bytes = margin;
        config.resume_margin_bytes = margin.saturating_mul(2);
    }
    for (category, bytes) in parse_category_budgets(overrides)? {
        config.budgets[category.index()] = bytes;
    }
    Ok(Some(config))
}

/// The paths of the managed root, by category.
pub struct ManagedPaths {
    pub db_dir: PathBuf,
    pub consensus_db_dir: PathBuf,
    pub meta_db_dir: PathBuf,
    pub utxoindex_db_dir: PathBuf,
    pub log_dir: Option<PathBuf>,
    pub wal_dir: Option<PathBuf>,
}

/// Build, assign, install and measure the budget. Returns it (the daemon keeps the handle for the
/// reclaimers and the service it registers later).
pub fn install_storage_budget(config: StorageBudgetConfig, paths: &ManagedPaths) -> Arc<StorageBudget> {
    for w in config.warnings() {
        warn!("[storage] {w}");
    }
    let root = config.root.clone();
    let budget = StorageBudget::new(config);
    budget.assign_path(paths.consensus_db_dir.clone(), StorageCategory::ActiveDb);
    budget.assign_path(paths.meta_db_dir.clone(), StorageCategory::ActiveDb);
    budget.assign_path(paths.utxoindex_db_dir.clone(), StorageCategory::ActiveDb);
    budget.assign_path(root.join("palw-retention"), StorageCategory::PalwRetained);
    budget.assign_path(root.join("palw-panel"), StorageCategory::PalwRetained);
    budget.assign_path(root.join(STORAGE_TMP_DIR), StorageCategory::IbdSpill);
    if let Some(log_dir) = &paths.log_dir {
        budget.assign_path(log_dir.clone(), StorageCategory::Logs);
    }
    if let Some(wal_dir) = &paths.wal_dir {
        // An external WAL dir is still the node's bytes; it is measured wherever it lives.
        budget.assign_path(wal_dir.clone(), StorageCategory::WalMaintenance);
    }
    let _ = std::fs::create_dir_all(root.join(STORAGE_TMP_DIR));
    if kaspa_utils::storage_budget::install_global(Arc::clone(&budget)).is_err() {
        warn!("[storage] a storage budget was already installed in this process; the first one stays in force");
    }
    let snapshot = budget.refresh();
    info!(
        "[storage] budget in force on {}: cap {} bytes, safety margin {} (resume at {}); {}",
        root.display(),
        budget.config().cap_bytes,
        budget.config().safety_margin_bytes,
        budget.config().resume_margin_bytes,
        snapshot.summary_line()
    );
    if snapshot.charge_total > budget.config().cap_bytes {
        warn!(
            "[storage] the managed root already holds {} bytes, above the cap of {} — the node starts PAUSED and will \
             resume once reclaim brings it under the resume margin",
            snapshot.charge_total,
            budget.config().cap_bytes
        );
    }
    budget
}

/// Register the node-wide reclaimers (consensus dirs are registered by the caller, which holds
/// the factory; the retention janitor registers itself).
pub fn register_basic_reclaimers(budget: &Arc<StorageBudget>, log_dir: Option<PathBuf>) {
    let tmp = budget.root().join(STORAGE_TMP_DIR);
    budget.register_reclaimer(FnReclaimer::new("abandoned-temp-files", move |_mode| reclaim_temp_dir(&tmp, STORAGE_TMP_HORIZON)));
    // Also the OS temp dir's leftovers of earlier (uncapped) runs of this node — spill files are
    // unlinked at creation on unix, so a visible one is a crash leftover — and the pruning-proof
    // builder's temp DBs (`$TMP/rusty-kaspa/*`), which a crash can orphan.
    budget.register_reclaimer(FnReclaimer::new("os-temp-leftovers", |_mode| reclaim_os_temp_leftovers(&std::env::temp_dir())));
    if let Some(log_dir) = log_dir {
        budget.register_reclaimer(FnReclaimer::new("log-archives", move |mode| {
            reclaim_log_archives(&log_dir, kaspa_core::log::LOG_FILE_MAX_ROLLS, mode)
        }));
    }
}

/// Spill files and temp DB dirs this node leaves in the OS temp dir, older than the horizon.
fn reclaim_os_temp_leftovers(tmp: &Path) -> u64 {
    let mut freed = 0u64;
    if let Ok(entries) = std::fs::read_dir(tmp) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.starts_with("misaka-palw-snapshot-") && name.ends_with(".spill")) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
            if meta.modified().ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age >= STORAGE_TMP_HORIZON)
                && std::fs::remove_file(entry.path()).is_ok()
            {
                freed = freed.saturating_add(kaspa_utils::storage_budget::allocated_bytes(&meta));
            }
        }
    }
    freed.saturating_add(reclaim_temp_dir(&tmp.join("rusty-kaspa"), STORAGE_TMP_HORIZON))
}

/// The bytes one log rotation needs while it writes the compressed archive next to the live file —
/// held for the process lifetime in the logs category (log4rs rotates inside the logger, where no
/// reservation can be taken at the moment of rotation).
pub fn log_rotation_reservation(budget: &Arc<StorageBudget>) -> Option<StorageReservation> {
    match budget.reserve(StorageCategory::Logs, kaspa_core::log::LOG_FILE_MAX_SIZE) {
        Ok(r) => Some(r),
        Err(refusal) => {
            warn!("[storage] no rotation headroom could be reserved for the log files: {refusal}");
            None
        }
    }
}

/// **The monitor**: measures the managed root every [`STORAGE_SCAN_PERIOD`] and drives the
/// low-space pause. The scan runs on a blocking thread.
pub struct StorageBudgetService {
    budget: Arc<StorageBudget>,
    _standing: Vec<StorageReservation>,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl StorageBudgetService {
    pub fn new(budget: Arc<StorageBudget>, standing: Vec<StorageReservation>) -> Self {
        Self { budget, _standing: standing, shutdown: Default::default() }
    }

    async fn worker(self: &Arc<Self>) {
        let mut last_summary = std::time::Instant::now();
        let mut was_paused = self.budget.is_paused();
        loop {
            let budget = Arc::clone(&self.budget);
            let snapshot = match tokio::task::spawn_blocking(move || budget.refresh()).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("[storage] the measurement task failed: {e}");
                    continue;
                }
            };
            if snapshot.paused != was_paused || last_summary.elapsed() >= STORAGE_SUMMARY_PERIOD {
                if snapshot.paused {
                    warn!("[storage] {}", snapshot.summary_line());
                } else {
                    info!("[storage] {}", snapshot.summary_line());
                }
                last_summary = std::time::Instant::now();
                was_paused = snapshot.paused;
            }
            tokio::select! {
                _ = tokio::time::sleep(STORAGE_SCAN_PERIOD) => {}
                _ = self.shutdown.listener.clone() => break,
            }
        }
    }
}

impl AsyncService for StorageBudgetService {
    fn ident(self: Arc<Self>) -> &'static str {
        STORAGE_BUDGET_SERVICE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", STORAGE_BUDGET_SERVICE);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", STORAGE_BUDGET_SERVICE);
            Ok(())
        })
    }
}

/// The reclaim mode's byte need (0 for obsolete-only).
pub fn reclaim_need(mode: ReclaimMode) -> u64 {
    match mode {
        ReclaimMode::Obsolete => 0,
        ReclaimMode::UnderPressure { need } => need,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_overrides_and_the_cap_parse_and_unknown_categories_are_refused() {
        let root = PathBuf::from("/managed");
        assert!(storage_budget_config(None, None, &[], root.clone()).unwrap().is_none(), "no cap, no budget");
        let config = storage_budget_config(
            Some(30_000_000_000),
            Some(500_000_000),
            &["palw-retained=3_000_000_000".into(), "logs=1000000000".into()],
            root.clone(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(config.budgets[StorageCategory::PalwRetained.index()], 3_000_000_000);
        assert_eq!(config.budgets[StorageCategory::Logs.index()], 1_000_000_000);
        assert_eq!(config.budgets[StorageCategory::ActiveDb.index()], 10_497_346_816);
        assert_eq!((config.safety_margin_bytes, config.resume_margin_bytes), (500_000_000, 1_000_000_000));
        assert!(storage_budget_config(Some(1), None, &["nope=1".into()], root.clone()).is_err());
        assert!(storage_budget_config(Some(0), None, &[], root).is_err());
    }
}
