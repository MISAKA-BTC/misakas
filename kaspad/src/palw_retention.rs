//! **The retention janitor: a node's `palw-retention/` is pruned whatever the node was started as.**
//!
//! A producer writes an attempt capture for every block it publishes (~0.8 GB on a graph-v5 class)
//! and a free-prompt capture for every claim it executes; a panel keeps the materials it verified
//! under `foreign/`. The prune used to run only inside the producer's draw loop, so a node started
//! without `--palw-produce` — a panel seat, a producer restarted as one, a node still syncing —
//! kept every file it had, and once a Qwen3.6 draw took a minute instead of eighteen a producer wrote
//! ~30 GB an hour (2026-09-12: C's 387 GB filled in under two hours). A node an operator joins with
//! has a small SSD. So this runs on every ConsensusV2 node, once a minute from startup, with no flag:
//!
//! * **The time rule** ([`crate::palw_producer::retained_capture_prune_due_v1`]): an attempt capture
//!   goes at `--palw-attempt-retention-minutes` (60 by default), a free-prompt capture at 48 h and
//!   never while the chain can still ask about it — past `palw_rcore_plus` that includes a `Final`
//!   claim a data-availability session can still open on (ADR-0152 DA-8; P2-7), until its
//!   `trace_retention_daa`.
//! * **The space rule**: while the volume has less free than [`retention_reserve_bytes_v1`],
//!   unowed attempt captures go oldest first, then unowed foreign copies. Every capture the chain
//!   can still demand is pinned under both rules. A possible future replay does not discharge the
//!   accepted obligation to retain committed material. Insufficient space is reported to the node;
//!   the janitor cannot create space by silently breaking an existing obligation.
//!
//! A capture is classified by its own bytes — `FPC1` opens the free-prompt envelope, anything else is
//! the attempt lane's bare capture — so a capture whose block the chain has not accepted yet, or never
//! will, is classified the same way as one it holds.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwClaimSourceV2};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    info,
    task::service::{AsyncService, AsyncServiceFuture},
    trace, warn,
};
use kaspa_hashes::Hash64;

const PALW_RETENTION: &str = "palw-retention";

/// The floor of the space rule: never let the retention volume's free space fall under 8 GiB …
pub(crate) const PALW_RETENTION_MIN_FREE_BYTES: u64 = 8 << 30;
/// … or under this share of the volume, in permille, whichever is larger.
pub(crate) const PALW_RETENTION_MIN_FREE_PERMILLE: u64 = 50;
/// A `.partial` file is a write in progress; one older than this was abandoned (a crash, an ENOSPC)
/// and is removed.
const PALW_RETENTION_PARTIAL_HORIZON: Duration = Duration::from_secs(3600);
/// How often the janitor looks.
const PALW_RETENTION_PERIOD: Duration = Duration::from_secs(60);
/// The free-prompt capture's envelope magic (`palw_fp_capture_encode_v1`).
const FP_CAPTURE_MAGIC: &[u8; 4] = b"FPC1";

// Lead and rider publications stay registered through their in-process handoff.
// The guard can cross asynchronous submission without holding the mutex.
static ACTIVE_ATTEMPT_PUBLICATIONS: Mutex<std::collections::BTreeMap<Hash64, usize>> = Mutex::new(std::collections::BTreeMap::new());

pub(crate) struct AttemptPublicationGuard(Hash64);

pub(crate) fn pin_attempt_publication(claim: Hash64) -> AttemptPublicationGuard {
    *ACTIVE_ATTEMPT_PUBLICATIONS.lock().unwrap().entry(claim).or_default() += 1;
    AttemptPublicationGuard(claim)
}

impl AttemptPublicationGuard {
    pub(crate) fn claim(&self) -> Hash64 {
        self.0
    }
}

#[cfg(test)]
pub(crate) fn attempt_publication_is_pinned(claim: Hash64) -> bool {
    ACTIVE_ATTEMPT_PUBLICATIONS.lock().unwrap().contains_key(&claim)
}

impl Drop for AttemptPublicationGuard {
    fn drop(&mut self) {
        let mut active = ACTIVE_ATTEMPT_PUBLICATIONS.lock().unwrap();
        let count = active.get_mut(&self.0).expect("publication guard must be registered");
        *count -= 1;
        if *count == 0 {
            active.remove(&self.0);
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RetainedPublication {
    Replace,
    KeepExisting,
    DurableReplace,
}

/// Publish complete bytes from a private staging name in the same directory.
/// Callers retain their claim pin for the full write and publication.
pub(crate) fn publish_retained_file(path: &Path, bytes: &[u8], mode: RetainedPublication) -> std::io::Result<bool> {
    use std::io::{Error, ErrorKind, Write};
    let dir = path.parent().ok_or_else(|| Error::other("retention directory absent"))?;
    let name = path.file_name().ok_or_else(|| Error::other("retention filename absent"))?;
    let existing = || std::fs::symlink_metadata(path).map(|meta| meta.is_file());
    if matches!(mode, RetainedPublication::KeepExisting) {
        match existing() {
            Ok(true) => return Ok(false),
            Ok(false) => return Err(Error::new(ErrorKind::AlreadyExists, "retained path is not a regular file")),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    std::fs::create_dir_all(dir)?;
    let mut builder = tempfile::Builder::new();
    builder.prefix(name).suffix(".partial");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut pending = builder.tempfile_in(dir)?;
    pending.write_all(bytes)?;
    if matches!(mode, RetainedPublication::DurableReplace) {
        pending.as_file().sync_all()?;
    }
    if matches!(mode, RetainedPublication::KeepExisting) {
        return match pending.persist_noclobber(path) {
            Ok(_) => Ok(true),
            Err(e) if e.error.kind() == ErrorKind::AlreadyExists && matches!(existing(), Ok(true)) => Ok(false),
            Err(e) => Err(e.error),
        };
    }
    pending.persist(path).map_err(|e| e.error)?;
    if matches!(mode, RetainedPublication::DurableReplace) {
        sync_retained_directory(dir)?;
    }
    Ok(true)
}

fn sync_retained_directory(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)?.sync_all()?;
        if let Some(parent) = dir.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::File::open(parent)?.sync_all()?;
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Reuse an exact replay without staging a second copy under storage pressure.
/// Re-sync the file and directory in case an earlier publication failed there.
pub(crate) fn ensure_retained_replay<T>(
    path: &Path,
    bytes: &[u8],
    reserve: impl FnOnce() -> std::io::Result<T>,
) -> std::io::Result<bool> {
    use std::io::{ErrorKind, Read};

    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() == bytes.len() as u64 => {
            let snapshot = RetainedFileV1::new(path.to_path_buf(), &meta);
            let mut file = std::fs::File::open(path)?;
            if snapshot.matches(&file.metadata()?) {
                let mut buf = [0; 16 * 1024];
                let mut identical = true;
                for expected in bytes.chunks(buf.len()) {
                    match file.read_exact(&mut buf[..expected.len()]) {
                        Ok(()) if &buf[..expected.len()] == expected => {}
                        Ok(()) => {
                            identical = false;
                            break;
                        }
                        Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                            identical = false;
                            break;
                        }
                        Err(e) => return Err(e),
                    }
                }
                if identical && snapshot.matches(&file.metadata()?) {
                    file.sync_all()?;
                    let dir = path.parent().ok_or_else(|| std::io::Error::other("replay directory absent"))?;
                    sync_retained_directory(dir)?;
                    if std::fs::symlink_metadata(path).is_ok_and(|meta| snapshot.matches(&meta)) {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(meta) if !meta.is_file() => return Err(std::io::Error::other("replay path is not a regular file")),
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let _storage = reserve()?;
    publish_retained_file(path, bytes, RetainedPublication::DurableReplace)
}

/// **The free space the space rule keeps**: `max(8 GiB, 5 % of the volume)`.
pub(crate) fn retention_reserve_bytes_v1(total_bytes: u64) -> u64 {
    PALW_RETENTION_MIN_FREE_BYTES.max(total_bytes / 1000 * PALW_RETENTION_MIN_FREE_PERMILLE)
}

/// What one retained claim — its `.material` and `.answer`, or one foreign copy — costs, and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedClaimV1 {
    pub claim: Hash64,
    pub files: Vec<RetainedFileV1>,
    pub bytes: u64,
    /// The newest of its files' ages (`None` = unreadable, treated as old).
    pub age: Option<Duration>,
    pub kind: RetainedKindV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedFileV1 {
    path: PathBuf,
    bytes: u64,
    version: Option<FileVersion>,
}

#[cfg(unix)]
type FileVersion = (u64, u64, i64, i64);
#[cfg(not(unix))]
type FileVersion = (std::time::SystemTime, std::time::SystemTime);

impl RetainedFileV1 {
    fn version(meta: &std::fs::Metadata) -> Option<FileVersion> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some((meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()))
        }
        #[cfg(not(unix))]
        {
            Some((meta.created().ok()?, meta.modified().ok()?))
        }
    }

    pub(crate) fn new(path: PathBuf, meta: &std::fs::Metadata) -> Self {
        Self { path, bytes: meta.len(), version: Self::version(meta) }
    }

    /// Serialize the final file check with in-process publishers.
    pub(crate) fn unlink_if_current(&self, claim: Option<Hash64>) -> bool {
        let active = ACTIVE_ATTEMPT_PUBLICATIONS.lock().unwrap();
        if claim.is_some_and(|claim| active.contains_key(&claim)) {
            return false;
        }
        std::fs::symlink_metadata(&self.path).is_ok_and(|meta| self.matches(&meta)) && std::fs::remove_file(&self.path).is_ok()
    }

    fn matches(&self, meta: &std::fs::Metadata) -> bool {
        meta.is_file() && meta.len() == self.bytes && self.version.is_some() && Self::version(meta) == self.version
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedKindV1 {
    /// This node's own attempt capture: re-made by replaying the block's job when a court asks.
    Attempt,
    /// This node's own free-prompt capture (`FPC1`): the one copy anywhere.
    FreePrompt,
    /// A material this seat verified for another producer (`foreign/`): a copy, re-verified by replay.
    Foreign,
}

/// **The chain's view of a retained claim at the tip**, as the prune reads it: its source and phase,
/// and whether R-core's data-availability court can still demand a unit of it — `da_owed`, past
/// `palw_rcore_plus` only: [`kaspa_consensus_core::palw_da_rcore_v1::palw_da_material_owed_v1`]
/// (ADR-0152 DA-8; Phase 2, P2-7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedChainViewV1 {
    pub source: PalwClaimSourceV2,
    pub phase: PalwClaimPhaseV2,
    pub da_owed: bool,
}

/// **Which retained claims go now**, as indexes into `claims`, and how many bytes the space rule still
/// could not free. Pure: `chain` is the claim's view at the tip (`None` = unknown), and
/// `free`/`reserve` are the volume's free bytes and the floor the space rule keeps.
///
/// **P2-7 (ADR-0152 DA-4, DA-7, X7):** past `palw_rcore_plus` a session opens on a `Final` claim too
/// (`FinalRow`: a default is S3, the row burned and `min(25% · C, 3 G)`), so a free-prompt capture —
/// the one copy anywhere — is never due while `da_owed`, whatever its phase; and under the space
/// rule the foreign copies a session can still demand are pinned alongside the producer's own committed capture. A covering signer answers from
/// its copy; potential replay does not replace this accepted retention obligation.
#[allow(dead_code)] // dormant or test-only: kept, not yet wired to a caller in this build
pub(crate) fn retention_prune_plan_v1(
    claims: &[RetainedClaimV1],
    chain: impl Fn(&Hash64) -> Option<RetainedChainViewV1>,
    attempt_horizon: Duration,
    free: u64,
    reserve: u64,
) -> (Vec<usize>, u64) {
    retention_prune_plan_with_material_v1(claims, |row| chain(&row.claim), |_| Ok(false), attempt_horizon, free, reserve)
}

/// The material oracle also covers kernel/provider subjects without native
/// claim rows. Only a successful read can clear a material obligation.
fn retention_prune_plan_with_material_v1(
    claims: &[RetainedClaimV1],
    chain: impl Fn(&RetainedClaimV1) -> Option<RetainedChainViewV1>,
    material_owed: impl Fn(&Hash64) -> Result<bool, String>,
    attempt_horizon: Duration,
    free: u64,
    reserve: u64,
) -> (Vec<usize>, u64) {
    let mut doomed = Vec::new();
    let mut pressure_candidates = Vec::new();
    let mut freed = 0u64;
    for (i, c) in claims.iter().enumerate() {
        if !matches!(material_owed(&c.claim), Ok(false)) {
            continue;
        }
        let view = chain(c);
        if view.as_ref().is_some_and(|view| view.da_owed) {
            continue;
        }
        let due = match c.kind {
            // The attempt lane's bytes say what the claim is, whatever the chain knows yet.
            RetainedKindV1::Attempt => crate::palw_producer::retained_capture_prune_due_v1(
                c.age,
                Some((&PalwClaimSourceV2::Attempt, &PalwClaimPhaseV2::Provisional)),
                attempt_horizon,
            ),
            RetainedKindV1::FreePrompt => crate::palw_producer::retained_capture_prune_due_v1(
                c.age,
                view.as_ref().map(|view| (&view.source, &view.phase)),
                attempt_horizon,
            ),
            // The panel prunes its foreign copies by age as it writes them; here they only yield space.
            RetainedKindV1::Foreign => false,
        };
        if due {
            doomed.push(i);
            freed = freed.saturating_add(c.bytes);
        } else if free < reserve && c.kind != RetainedKindV1::FreePrompt {
            pressure_candidates.push(i);
        }
    }
    // The space rule picks the oldest eligible attempts first, then eligible
    // foreign copies. DA-owed copies were excluded above in both cases.
    let mut short = reserve.saturating_sub(free.saturating_add(freed));
    if short > 0 {
        pressure_candidates.sort_unstable_by_key(|&i| {
            let c = &claims[i];
            (c.kind == RetainedKindV1::Foreign, std::cmp::Reverse(c.age.unwrap_or(Duration::MAX)), i)
        });
        for i in pressure_candidates {
            if short == 0 {
                break;
            }
            doomed.push(i);
            short = short.saturating_sub(claims[i].bytes);
        }
    }
    (doomed, short)
}

fn prune_retained_claims_v1(
    claims: &[RetainedClaimV1],
    doomed: Vec<usize>,
    material_owed: impl Fn(&Hash64) -> Result<bool, String>,
) -> (usize, u64) {
    let mut removed = 0usize;
    let mut bytes = 0u64;
    let mut unchanged = Vec::new();
    'claims: for i in doomed {
        let c = &claims[i];
        // Keep registration serialized with the final check and unlink.
        let active = ACTIVE_ATTEMPT_PUBLICATIONS.lock().unwrap();
        if active.contains_key(&c.claim) || !matches!(material_owed(&c.claim), Ok(false)) {
            continue;
        }
        // A publication can replace a file and release its guard between the
        // scan and this lock. Discard the old plan for the whole capture.
        unchanged.clear();
        for file in &c.files {
            match std::fs::symlink_metadata(&file.path) {
                Ok(meta) if file.matches(&meta) => unchanged.push(file),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => continue 'claims,
            }
        }
        let mut removed_file = false;
        for file in &unchanged {
            match std::fs::remove_file(&file.path) {
                Ok(()) => {
                    removed_file = true;
                    bytes = bytes.saturating_add(file.bytes);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => trace!("[{PALW_RETENTION}] cannot prune {}: {e}", file.path.display()),
            }
        }
        removed += usize::from(removed_file);
    }
    (removed, bytes)
}

/// Read `dir` (and `dir/foreign`) into retained claims; `.partial` files older than an hour are
/// returned separately, to be removed.
fn scan_retention_v1(dir: &Path, now: std::time::SystemTime) -> (Vec<RetainedClaimV1>, Vec<(Option<Hash64>, RetainedFileV1)>) {
    let mut own: std::collections::BTreeMap<Hash64, RetainedClaimV1> = std::collections::BTreeMap::new();
    let mut claims = Vec::new();
    let mut abandoned = Vec::new();
    let age_of = |meta: &std::fs::Metadata| meta.modified().ok().map(|t| now.duration_since(t).unwrap_or_default());
    for (sub, foreign) in [(dir.to_path_buf(), false), (dir.join("foreign"), true)] {
        let Ok(entries) = std::fs::read_dir(&sub) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else { continue };
            let age = age_of(&meta);
            if name.ends_with(".partial") {
                if age.is_none_or(|a| a >= PALW_RETENTION_PARTIAL_HORIZON) {
                    let claim = name.split_once('.').and_then(|(stem, _)| stem.parse().ok());
                    abandoned.push((claim, RetainedFileV1::new(path, &meta)));
                }
                continue;
            }
            let (stem, material) = match (name.strip_suffix(".material"), name.strip_suffix(".answer")) {
                (Some(stem), _) => (stem, true),
                (None, Some(stem)) => (stem, false),
                _ if foreign => match name.strip_suffix(".replay") {
                    Some(stem) => (stem, false),
                    None => continue,
                },
                _ => continue,
            };
            let Ok(claim) = stem.parse::<Hash64>() else { continue };
            if foreign {
                claims.push(RetainedClaimV1 {
                    claim,
                    files: vec![RetainedFileV1::new(path, &meta)],
                    bytes: meta.len(),
                    age,
                    kind: RetainedKindV1::Foreign,
                });
                continue;
            }
            let kind = if material && starts_with_fp_magic(&path) { RetainedKindV1::FreePrompt } else { RetainedKindV1::Attempt };
            let row = own.entry(claim).or_insert(RetainedClaimV1 {
                claim,
                files: Vec::new(),
                bytes: 0,
                age: None,
                kind: RetainedKindV1::Attempt,
            });
            // The claim is as young as its youngest file; an unreadable age reads as old.
            row.age = match (row.files.is_empty(), row.age, age) {
                (true, _, age) => age,
                (false, Some(a), Some(b)) => Some(a.min(b)),
                (false, Some(a), None) => Some(a),
                (false, None, b) => b,
            };
            row.files.push(RetainedFileV1::new(path, &meta));
            row.bytes = row.bytes.saturating_add(meta.len());
            if material {
                row.kind = kind;
            }
        }
    }
    // An `.answer` alone is classified by the chain view later only if it matters; its bytes are
    // small, and it goes with the attempt rule by default — as it always went with its material.
    claims.extend(own.into_values());
    (claims, abandoned)
}

fn starts_with_fp_magic(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let mut head = [0u8; 4];
    file.read_exact(&mut head).is_ok() && &head == FP_CAPTURE_MAGIC
}

/// The free and total bytes of the volume holding `path` — the deepest mount point that contains it.
pub(crate) fn volume_space_v1(path: &Path) -> Option<(u64, u64)> {
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
            if best.is_none_or(|(best_len, _, _)| len > best_len) {
                best = Some((len, disk.available_space(), disk.total_space()));
            }
        }
    }
    best.map(|(_, available, total)| (available, total))
}

/// **The service.** Registered by the daemon on every ConsensusV2 node, whatever else it runs.
pub struct PalwRetentionJanitor {
    consensus_manager: Arc<ConsensusManager>,
    dir: PathBuf,
    attempt_horizon: Duration,
    /// `Params::palw_rcore_plus` (`palw_rcore_plus_fence`): past it the DA court's window is read
    /// off the claim's retention, not its phase (P2-7).
    rcore_plus: Option<kaspa_consensus_core::config::params::ForkActivation>,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl PalwRetentionJanitor {
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        dir: PathBuf,
        attempt_horizon: Duration,
        rcore_plus: Option<kaspa_consensus_core::config::params::ForkActivation>,
    ) -> Self {
        Self { consensus_manager, dir, attempt_horizon, rcore_plus, shutdown: kaspa_utils::triggers::SingleTrigger::default() }
    }

    /// One pass: scan, plan, remove. Returns what it removed and what the space rule could not free.
    ///
    /// Under a storage budget (`--storage-cap-bytes`, G14) the space rule plans against the budget
    /// instead of the raw volume: `free` is the budget's headroom and the floor is its resume margin
    /// plus whatever the retained material holds above its category budget (a reclaim TARGET — the
    /// plan still pins every capture the chain can demand, so a target it cannot reach is reported,
    /// never met by deleting owed material).
    fn pass(&self) -> (usize, u64, u64) {
        let (free, reserve) = match kaspa_utils::storage_budget::global() {
            Some(budget) => {
                use kaspa_utils::storage_budget::StorageCategory;
                let snapshot = budget.snapshot();
                let i = StorageCategory::PalwRetained.index();
                let over_target = snapshot.scanned[i].saturating_sub(budget.config().budgets[i]);
                (snapshot.headroom, budget.config().resume_margin_bytes.saturating_add(over_target))
            }
            None => {
                let (free, total) = volume_space_v1(&self.dir).unwrap_or((u64::MAX, 0));
                (free, if total == 0 { 0 } else { retention_reserve_bytes_v1(total) })
            }
        };
        self.pass_with(free, reserve)
    }

    /// **The storage budget's reclaimer** (G14): the time rule and abandoned partial writes in
    /// `Obsolete` mode; under pressure, also the space rule for `need` bytes — oldest re-makeable,
    /// UNOWED captures only (`retention_prune_plan_with_material_v1` pins everything the chain can
    /// still demand). Returns the bytes removed.
    pub fn reclaim_for_budget(&self, mode: kaspa_utils::storage_budget::ReclaimMode) -> u64 {
        let need = crate::storage_capacity::reclaim_need(mode);
        // `free = need, reserve = 2·need` asks the space rule for exactly `need` bytes (0: time rule only).
        let (_, bytes, short) = self.pass_with(need, need.saturating_mul(2));
        if short > 0 {
            warn!(
                "[{PALW_RETENTION}] the storage budget asked for {need} bytes; {short} bytes could not be freed without                  deleting material the chain can still demand, so they were not"
            );
        }
        bytes
    }

    fn pass_with(&self, free: u64, reserve: u64) -> (usize, u64, u64) {
        let now = std::time::SystemTime::now();
        let (claims, abandoned) = scan_retention_v1(&self.dir, now);
        let removed_abandoned = abandoned.iter().filter(|(claim, file)| file.unlink_if_current(*claim)).count();
        if claims.is_empty() {
            return (removed_abandoned, 0, 0);
        }
        let session = self.consensus_manager.consensus().unguarded_session();
        let now_daa = session.get_virtual_daa_score();
        let rcore = self.rcore_plus.is_some_and(|fence| fence.is_active(now_daa));
        // The resident obligation oracle covers DA for all file kinds.
        // Only a free-prompt age decision still needs the legacy claim phase.
        let chain = |row: &RetainedClaimV1| {
            if row.kind != RetainedKindV1::FreePrompt {
                return None;
            }
            session.palw_derived_artifacts_v1(row.claim).map(|(state, _, _)| RetainedChainViewV1 {
                da_owed: rcore && kaspa_consensus_core::palw_da_rcore_v1::palw_da_material_owed_v1(&state, now_daa),
                source: state.source,
                phase: state.phase,
            })
        };
        let owed = |claim: &Hash64| {
            if ACTIVE_ATTEMPT_PUBLICATIONS.lock().unwrap().contains_key(claim) {
                return Ok(true);
            }
            if rcore {
                session.with_consensus_api_v1(|consensus| consensus.palw_material_retention_owed_v1(*claim))
            } else {
                Ok(session.palw_material_income_hold_until_v1(*claim).is_some_and(|until| until == u64::MAX || now_daa < until))
            }
        };
        let (doomed, _) = retention_prune_plan_with_material_v1(&claims, chain, owed, self.attempt_horizon, free, reserve);
        let (removed, bytes) = prune_retained_claims_v1(&claims, doomed, |claim| {
            // The selected branch or the active consensus can change while planning.
            // Recheck the resident obligation just before deleting a file group.
            if let Some(fence) = self.rcore_plus {
                let current = self.consensus_manager.consensus().unguarded_session();
                if rcore || fence.is_active(current.get_virtual_daa_score()) {
                    return current.with_consensus_api_v1(|consensus| consensus.palw_material_retention_owed_v1(*claim));
                }
            }
            Ok(false)
        });
        (removed + removed_abandoned, bytes, reserve.saturating_sub(free.saturating_add(bytes)))
    }

    async fn worker(self: &Arc<Self>) {
        info!(
            "[{PALW_RETENTION}] pruning {} every {} s: attempt captures after {} min, free-prompt captures after 48 h once \
             the chain is done with them, and oldest re-makeable captures first while the volume has under max(8 GiB, 5 %) free",
            self.dir.display(),
            PALW_RETENTION_PERIOD.as_secs(),
            self.attempt_horizon.as_secs() / 60
        );
        loop {
            let (removed, bytes, short) = self.pass();
            if removed > 0 {
                info!("[{PALW_RETENTION}] pruned {removed} retained file group(s), {} MB", bytes / 1_000_000);
            }
            if short > 0 {
                warn!(
                    "[{PALW_RETENTION}] the retention volume is still {} MB under its free-space floor: what is left is \
                     free-prompt captures the chain can still ask about, or other data on the volume",
                    short / 1_000_000
                );
            }
            tokio::select! {
                _ = tokio::time::sleep(PALW_RETENTION_PERIOD) => {}
                _ = self.shutdown.listener.clone() => break,
            }
        }
    }
}

impl AsyncService for PalwRetentionJanitor {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_RETENTION
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_RETENTION);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_RETENTION);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2 as R;

    const GIB: u64 = 1 << 30;
    const MIN: Duration = Duration::from_secs(60);

    fn claim(n: u64, kind: RetainedKindV1, bytes: u64, age_min: u64) -> RetainedClaimV1 {
        RetainedClaimV1 { claim: Hash64::from_u64_word(n), files: vec![], bytes, age: Some(MIN * age_min as u32), kind }
    }

    /// **The floor is 8 GiB, or 5 % of a volume big enough for that to be more.**
    #[test]
    fn material_only_kernel_subject_pins_old_own_and_foreign_files_without_a_native_claim() {
        let claims = vec![
            RetainedClaimV1 {
                claim: Hash64::from_u64_word(1),
                files: vec![],
                bytes: 40,
                age: Some(Duration::MAX),
                kind: RetainedKindV1::Attempt,
            },
            RetainedClaimV1 {
                claim: Hash64::from_u64_word(2),
                files: vec![],
                bytes: 30,
                age: Some(Duration::MAX),
                kind: RetainedKindV1::Foreign,
            },
        ];
        let (doomed, short) = retention_prune_plan_with_material_v1(&claims, |_| None, |_| Ok(true), 60 * MIN, 0, 100);
        assert!(doomed.is_empty());
        assert_eq!(short, 100, "owed material cannot satisfy space reclamation");
        let (doomed, short) = retention_prune_plan_with_material_v1(&claims, |_| None, |_| Ok(false), 60 * MIN, 0, 100);
        assert_eq!(doomed, vec![0, 1]);
        assert_eq!(short, 30, "both copies become reclaimable after their exact release");
    }

    #[test]
    fn retention_read_failure_pins_files_during_planning_and_immediately_before_pruning() {
        let dir = tempfile::tempdir().unwrap();
        let mut claims = [
            claim(1, RetainedKindV1::Attempt, 40, 90),
            claim(2, RetainedKindV1::FreePrompt, 20, 5000),
            claim(3, RetainedKindV1::Foreign, 30, 900),
        ];
        for claim in &mut claims {
            let path = dir.path().join(format!("{}.material", claim.claim));
            std::fs::write(&path, vec![0; claim.bytes as usize]).unwrap();
            claim.files.push(RetainedFileV1::new(path.clone(), &std::fs::metadata(path).unwrap()));
        }
        let plan = |owed: fn(&Hash64) -> Result<bool, String>| {
            retention_prune_plan_with_material_v1(&claims, |_| None, owed, 60 * MIN, 0, 100)
        };
        let unavailable = |_: &Hash64| Err("accepted snapshot unavailable".into());
        let unowed = |_: &Hash64| Ok(false);
        assert_eq!(plan(unavailable), (vec![], 100));

        let (doomed, short) = plan(unowed);
        assert_eq!((doomed.as_slice(), short), (&[0, 1, 2][..], 10));
        assert_eq!(prune_retained_claims_v1(&claims, doomed, |_| Err("selected branch changed".into())), (0, 0));
        assert!(claims.iter().all(|claim| claim.files[0].path.is_file()));

        let (doomed, _) = plan(unowed);
        assert_eq!(prune_retained_claims_v1(&claims, doomed, unowed), (3, 90));
        assert!(claims.iter().all(|claim| !claim.files[0].path.exists()));
    }

    #[test]
    fn active_lead_publication_keeps_material_through_pressure_reclaim() {
        let dir = tempfile::tempdir().unwrap();
        let mut row = claim(15, RetainedKindV1::Attempt, 3, 0);
        let material = dir.path().join("claim.material");
        // The deletion plan predates the publication. Both guards share the
        // same claim, and an early exit drops only its own guard.
        let first = pin_attempt_publication(row.claim);
        let second = pin_attempt_publication(row.claim);
        std::fs::write(&material, b"new").unwrap();
        row.files.push(RetainedFileV1::new(material.clone(), &std::fs::metadata(&material).unwrap()));
        drop(first);
        assert_eq!(prune_retained_claims_v1(&[row.clone()], vec![0], |_| Ok(false)), (0, 0));
        assert_eq!(std::fs::read(&material).unwrap(), b"new");
        drop(second);
        assert_eq!(prune_retained_claims_v1(&[row], vec![0], |_| Ok(false)), (1, 3));
    }

    #[test]
    fn pressure_plan_can_reclaim_other_material_while_a_lead_is_publishing() {
        let active = claim(16, RetainedKindV1::Attempt, 40, 90);
        let other = claim(17, RetainedKindV1::Attempt, 50, 30);
        let _pin = pin_attempt_publication(active.claim);
        let owed = |claim: &Hash64| Ok(ACTIVE_ATTEMPT_PUBLICATIONS.lock().unwrap().contains_key(claim));
        assert_eq!(retention_prune_plan_with_material_v1(&[active, other], |_| None, owed, 60 * MIN, 0, 40), (vec![1], 0));
    }

    #[test]
    fn pressure_plan_reads_each_eligible_row_once_and_preserves_order() {
        use std::cell::Cell;

        let claims = [
            claim(101, RetainedKindV1::Foreign, 10, 120),
            claim(102, RetainedKindV1::Attempt, 10, 30),
            claim(103, RetainedKindV1::Attempt, 10, 30),
            claim(104, RetainedKindV1::Attempt, 10, 90),
            claim(105, RetainedKindV1::Foreign, 10, 130),
        ];
        let chain_reads = Cell::new(0);
        let obligation_reads = Cell::new(0);
        let chain = |_: &RetainedClaimV1| {
            chain_reads.set(chain_reads.get() + 1);
            None
        };
        let owed = |_: &Hash64| {
            obligation_reads.set(obligation_reads.get() + 1);
            Ok(false)
        };
        assert_eq!(retention_prune_plan_with_material_v1(&claims, chain, owed, 60 * MIN, 0, 40), (vec![3, 1, 2, 4], 0));
        assert_eq!((chain_reads.get(), obligation_reads.get()), (claims.len(), claims.len()));
    }

    #[test]
    fn replay_retry_reuses_exact_durable_bytes_without_reserving_space() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claim.replay");
        let bytes = vec![7; 32 * 1024 + 1];
        let refuse = || Err::<(), _>(std::io::Error::other("storage budget exhausted"));
        assert!(ensure_retained_replay(&path, &bytes, refuse).is_err());
        assert!(!path.exists());
        assert!(ensure_retained_replay(&path, &bytes, || Ok(())).unwrap());
        assert!(!ensure_retained_replay(&path, &bytes, refuse).unwrap());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        let mut different = bytes.clone();
        *different.last_mut().unwrap() ^= 1;
        assert!(ensure_retained_replay(&path, &different, refuse).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(ensure_retained_replay(&path, &different, || Ok(())).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), different);
    }

    #[cfg(unix)]
    #[test]
    fn replay_retry_rejects_symlinks_even_with_matching_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("existing");
        let replay = dir.path().join("claim.replay");
        std::fs::write(&existing, b"matching").unwrap();
        std::os::unix::fs::symlink(&existing, &replay).unwrap();
        assert!(ensure_retained_replay(&replay, b"matching", || Ok(())).is_err());
        assert_eq!(std::fs::read(existing).unwrap(), b"matching");
        assert!(std::fs::symlink_metadata(replay).unwrap().is_symlink());
    }

    #[test]
    fn licensed_replay_is_kept_separately_and_remains_subject_to_claim_gc() {
        let dir = tempfile::tempdir().unwrap();
        let foreign = dir.path().join("foreign");
        let claim = Hash64::from_u64_word(115);
        let material = foreign.join(format!("{claim}.material"));
        let replay = foreign.join(format!("{claim}.replay"));
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(&material, b"legacy truncated capture").unwrap();
        assert!(publish_retained_file(&replay, b"verified replay job", RetainedPublication::DurableReplace).unwrap());
        assert_eq!(std::fs::read(&material).unwrap(), b"legacy truncated capture");
        assert_eq!(std::fs::read(&replay).unwrap(), b"verified replay job");
        let (claims, _) = scan_retention_v1(dir.path(), std::time::SystemTime::now());
        assert_eq!(claims.len(), 2);
        assert!(claims.iter().all(|row| row.claim == claim && row.kind == RetainedKindV1::Foreign));
        let needed = |id: &Hash64| Ok(*id == claim);
        assert_eq!(retention_prune_plan_with_material_v1(&claims, |_| None, needed, 60 * MIN, 0, 1), (vec![], 1));
        let (doomed, _) = retention_prune_plan_v1(&claims, |_| None, 60 * MIN, 0, u64::MAX);
        assert_eq!(prune_retained_claims_v1(&claims, doomed, |_| Ok(false)).0, 2);
        assert!(!material.exists() && !replay.exists());
    }

    #[test]
    fn retained_publishers_never_expose_partial_or_overwrite_a_write_once_copy() {
        use std::sync::Barrier;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claim.material");
        let start = Barrier::new(8);
        let results = std::thread::scope(|scope| {
            let writers: Vec<_> = (0u8..8)
                .map(|byte| {
                    let (path, start) = (&path, &start);
                    scope.spawn(move || {
                        let bytes = vec![byte; 64 * 1024];
                        start.wait();
                        publish_retained_file(path, &bytes, RetainedPublication::KeepExisting).unwrap().then_some(byte)
                    })
                })
                .collect();
            writers.into_iter().filter_map(|writer| writer.join().unwrap()).collect::<Vec<_>>()
        });
        assert_eq!(results.len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), vec![results[0]; 64 * 1024]);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        assert!(publish_retained_file(&path, b"replacement", RetainedPublication::Replace).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn abandoned_scan_cannot_remove_an_active_or_replaced_stage() {
        let dir = tempfile::tempdir().unwrap();
        let claim = Hash64::from_u64_word(113);
        let path = dir.path().join(format!("{claim}.material.partial"));
        std::fs::write(&path, b"old").unwrap();
        let later = std::time::SystemTime::now() + PALW_RETENTION_PARTIAL_HORIZON;
        let (_, abandoned) = scan_retention_v1(dir.path(), later);
        let (id, file) = &abandoned[0];
        assert_eq!(*id, Some(claim));

        let pin = pin_attempt_publication(claim);
        assert!(!file.unlink_if_current(*id));
        let pending = dir.path().join("replacement");
        std::fs::write(&pending, b"new").unwrap();
        std::fs::rename(pending, &path).unwrap();
        drop(pin);
        assert!(!file.unlink_if_current(*id));
        assert_eq!(std::fs::read(&path).unwrap(), b"new");

        let later = std::time::SystemTime::now() + PALW_RETENTION_PARTIAL_HORIZON;
        let (_, abandoned) = scan_retention_v1(dir.path(), later);
        assert!(abandoned[0].1.unlink_if_current(abandoned[0].0));
        assert!(!path.exists());
    }

    #[test]
    fn a_capture_written_after_scan_start_is_new_for_age_pruning() {
        let dir = tempfile::tempdir().unwrap();
        let before_write = std::time::UNIX_EPOCH;
        let material = dir.path().join(format!("{}.material", Hash64::from_u64_word(112)));
        std::fs::write(material, b"new").unwrap();
        let (claims, _) = scan_retention_v1(dir.path(), before_write);
        assert_eq!(claims[0].age, Some(Duration::ZERO));
        assert_eq!(retention_prune_plan_v1(&claims, |_| None, 60 * MIN, 100, 0), (vec![], 0));
    }

    #[test]
    fn pressure_plan_cannot_delete_a_capture_replaced_after_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        let claim = Hash64::from_u64_word(111);
        let material = dir.path().join(format!("{claim}.material"));
        let answer = dir.path().join(format!("{claim}.answer"));
        std::fs::write(&material, b"old").unwrap();
        std::fs::write(&answer, b"answer").unwrap();
        let (claims, _) = scan_retention_v1(dir.path(), std::time::SystemTime::now());
        let (doomed, _) = retention_prune_plan_v1(&claims, |_| None, 60 * MIN, 0, 9);
        assert_eq!(doomed, vec![0]);

        let pin = pin_attempt_publication(claim);
        let replacement = dir.path().join("replacement");
        std::fs::write(&replacement, b"new").unwrap();
        std::fs::rename(replacement, &material).unwrap();
        drop(pin);
        assert_eq!(prune_retained_claims_v1(&claims, doomed, |_| Ok(false)), (0, 0));
        assert_eq!(std::fs::read(&material).unwrap(), b"new");
        assert_eq!(std::fs::read(&answer).unwrap(), b"answer");
    }

    #[test]
    fn retention_reclaim_reports_only_files_it_removed() {
        let dir = tempfile::tempdir().unwrap();
        let mut row = claim(1, RetainedKindV1::Attempt, 400, 90);
        let material = dir.path().join("claim.material");
        let missing = dir.path().join("gone.material");
        std::fs::write(&material, b"retained bytes").unwrap();
        std::fs::write(&missing, b"removed by another pass").unwrap();
        for path in [&material, &missing] {
            row.files.push(RetainedFileV1::new(path.clone(), &std::fs::metadata(path).unwrap()));
        }
        std::fs::remove_file(missing).unwrap();
        assert_eq!(prune_retained_claims_v1(&[row.clone()], vec![0], |_| Ok(false)), (1, 14));
        assert!(!material.exists());
        assert_eq!(prune_retained_claims_v1(&[row], vec![0], |_| Ok(false)), (0, 0));
    }

    #[test]
    fn the_reserve_is_eight_gib_or_five_percent() {
        assert_eq!(retention_reserve_bytes_v1(0), 8 * GIB);
        assert_eq!(retention_reserve_bytes_v1(100 * GIB), 8 * GIB, "5 % of 100 GiB is under the floor");
        assert_eq!(retention_reserve_bytes_v1(1000 * GIB), 50 * GIB, "5 % of a 1 TiB-ish volume");
    }

    /// **With room to spare only the clock prunes; out of room, the oldest attempt captures go first,
    /// then foreign copies, and a free-prompt capture never goes.**
    #[test]
    fn under_its_floor_the_volume_gives_up_attempt_captures_oldest_first_and_never_a_free_prompt_one() {
        let claims = vec![
            claim(1, RetainedKindV1::Attempt, 10 * GIB, 30), // young
            claim(2, RetainedKindV1::Attempt, 10 * GIB, 50), // older, still young
            claim(3, RetainedKindV1::Attempt, 10 * GIB, 90), // past the 60-minute horizon
            claim(4, RetainedKindV1::FreePrompt, 10 * GIB, 5000),
            claim(5, RetainedKindV1::Foreign, 10 * GIB, 100),
        ];
        let live = |_: &Hash64| {
            Some(RetainedChainViewV1 {
                source: PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() },
                phase: PalwClaimPhaseV2::Provisional,
                da_owed: false,
            })
        };
        // Plenty of room: the clock alone — only the attempt capture past its hour.
        let (doomed, short) = retention_prune_plan_v1(&claims, live, 60 * MIN, 100 * GIB, 8 * GIB);
        assert_eq!((doomed, short), (vec![2], 0));
        // 5 GiB free against an 8 GiB floor: the clock frees 10, which is enough.
        let (doomed, short) = retention_prune_plan_v1(&claims, live, 60 * MIN, 5 * GIB, 8 * GIB);
        assert_eq!((doomed, short), (vec![2], 0));
        // Nothing free against a 25 GiB floor: the expired one, then the oldest attempt, then the young one.
        let (doomed, short) = retention_prune_plan_v1(&claims, live, 60 * MIN, 0, 25 * GIB);
        assert_eq!((doomed, short), (vec![2, 1, 0], 0));
        // Against 45 GiB: every attempt capture and the foreign copy, and 5 GiB still short — the live
        // free-prompt capture is never taken, however full the disk.
        let (doomed, short) = retention_prune_plan_v1(&claims, live, 60 * MIN, 0, 45 * GIB);
        assert_eq!((doomed, short), (vec![2, 1, 0, 4], 5 * GIB));
        // A free-prompt claim the chain is done with goes on the clock (48 h), not on space.
        let done = |_: &Hash64| {
            Some(RetainedChainViewV1 {
                source: PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() },
                phase: PalwClaimPhaseV2::Voided { voided_daa: 1, reason: R::ReceiptTimeout },
                da_owed: false,
            })
        };
        let (doomed, _) = retention_prune_plan_v1(&claims, done, 60 * MIN, 100 * GIB, 8 * GIB);
        assert_eq!(doomed, vec![2, 3]);
    }

    /// **P2-7 (ADR-0152 DA-8): what R-core's court can still ask for is kept.** Past `palw_rcore_plus`
    /// a `Final` free-prompt claim is still accused (`FinalRow`, whose default is S3), so its capture —
    /// past the 48 h clock and `Final`, which below the fence releases it — stays while `da_owed`, and
    /// goes on the clock once the claim's retention has passed. Space pressure also keeps every owed
    /// own/foreign copy, and reports the remaining deficit.
    #[test]
    fn p2_7_a_capture_the_da_court_can_still_ask_for_is_kept() {
        let fp = PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() };
        let view =
            |da_owed: bool| RetainedChainViewV1 { source: fp.clone(), phase: PalwClaimPhaseV2::Final { final_daa: 9 }, da_owed };
        let claims = vec![claim(1, RetainedKindV1::FreePrompt, GIB, 5000)];
        let (doomed, _) = retention_prune_plan_v1(&claims, |_: &Hash64| Some(view(true)), 60 * MIN, 100 * GIB, 8 * GIB);
        assert!(doomed.is_empty(), "a Final claim a session can still open on keeps its capture");
        let (doomed, _) = retention_prune_plan_v1(&claims, |_: &Hash64| Some(view(false)), 60 * MIN, 100 * GIB, 8 * GIB);
        assert_eq!(doomed, vec![0], "past its retention (or below the fence) Final releases it at 48 h, as before");
        // The space rule can take the unowed copy, never the older owed one.
        let foreign = vec![claim(2, RetainedKindV1::Foreign, 10 * GIB, 100), claim(3, RetainedKindV1::Foreign, 10 * GIB, 900)];
        let owed = |claim: &Hash64| Some(view(*claim == Hash64::from_u64_word(3)));
        let (doomed, short) = retention_prune_plan_v1(&foreign, owed, 60 * MIN, 0, 5 * GIB);
        assert_eq!((doomed, short), (vec![0], 0), "the unowed copy frees the floor alone");
        let (doomed, short) = retention_prune_plan_v1(&foreign, owed, 60 * MIN, 0, 15 * GIB);
        assert_eq!((doomed, short), (vec![0], 5 * GIB), "space pressure cannot discharge the accepted material obligation");
        let unowed = |_: &Hash64| Some(view(false));
        let (doomed, _) = retention_prune_plan_v1(&foreign, unowed, 60 * MIN, 0, 5 * GIB);
        assert_eq!(doomed, vec![1], "with nothing owed, oldest first as before");
        // Both kinds of owed copy remain pinned. Replayability does not release a signed material duty.
        let attempt = |da_owed: bool| RetainedChainViewV1 {
            source: PalwClaimSourceV2::Attempt,
            phase: PalwClaimPhaseV2::Final { final_daa: 9 },
            da_owed,
        };
        let mixed = vec![claim(4, RetainedKindV1::Foreign, 10 * GIB, 900), claim(5, RetainedKindV1::Foreign, 10 * GIB, 100)];
        let owed_both = |claim: &Hash64| Some(if *claim == Hash64::from_u64_word(4) { view(true) } else { attempt(true) });
        let (doomed, short) = retention_prune_plan_v1(&mixed, owed_both, 60 * MIN, 0, 5 * GIB);
        assert_eq!((doomed, short), (vec![], 5 * GIB), "both committed copies remain owed");
        let (doomed, short) = retention_prune_plan_v1(&mixed, owed_both, 60 * MIN, 0, 15 * GIB);
        assert_eq!((doomed, short), (vec![], 15 * GIB), "neither accepted liability yields to the space rule");
        let own = vec![claim(6, RetainedKindV1::Attempt, 10 * GIB, 5000)];
        let (doomed, short) = retention_prune_plan_v1(&own, |_| Some(attempt(true)), 60 * MIN, 0, 15 * GIB);
        assert_eq!((doomed, short), (vec![], 15 * GIB), "an old own attempt is pinned by both age and pressure");
        let (doomed, short) = retention_prune_plan_v1(&own, |_| Some(attempt(false)), 60 * MIN, 0, 15 * GIB);
        assert_eq!((doomed, short), (vec![0], 5 * GIB), "the same capture yields once its actual obligation ends");
    }

    /// **The directory is read by its bytes and names**: an `FPC1` material is a free-prompt capture, a
    /// bare one an attempt capture with its answer beside it, `foreign/` holds copies, and a `.partial`
    /// older than an hour was abandoned.
    #[test]
    fn the_scan_classifies_by_bytes_and_pairs_material_with_answer() {
        let dir = std::env::temp_dir().join(format!("palw-retention-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("foreign")).unwrap();
        let a = Hash64::from_u64_word(1);
        let f = Hash64::from_u64_word(2);
        let g = Hash64::from_u64_word(3);
        std::fs::write(dir.join(format!("{a}.material")), [1u8, 2, 0, 0, 9]).unwrap();
        std::fs::write(dir.join(format!("{a}.answer")), [7u8; 3]).unwrap();
        std::fs::write(dir.join(format!("{f}.material")), b"FPC1rest").unwrap();
        std::fs::write(dir.join("foreign").join(format!("{g}.material")), [5u8; 6]).unwrap();
        let partial = dir.join(format!("{a}.material.partial"));
        std::fs::write(&partial, [0u8; 2]).unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(2 * 3600);
        let (claims, abandoned) = scan_retention_v1(&dir, later);
        assert_eq!(abandoned.len(), 1);
        assert_eq!(abandoned[0].0, Some(a));
        assert_eq!(abandoned[0].1.path, partial);
        let by = |c: &Hash64| claims.iter().find(|x| &x.claim == c).cloned().unwrap();
        assert_eq!((by(&a).kind, by(&a).bytes, by(&a).files.len()), (RetainedKindV1::Attempt, 8, 2));
        assert_eq!(by(&f).kind, RetainedKindV1::FreePrompt);
        assert_eq!(by(&g).kind, RetainedKindV1::Foreign);
        assert!(by(&a).age.is_some_and(|age| age >= Duration::from_secs(7000)), "two hours after the writes");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
