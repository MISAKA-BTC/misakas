//! Explicit, local-only PALW Object-v2 ingress.
//!
//! The producer atomically places `<job>.palwda` and then `<job>.json` in `incoming/`. This service
//! claims both into `processing/`, enforces regular-file/no-symlink/owner-only bounds, and invokes the
//! same full selected-chain admission path as peer recovery. Success is moved to `archive/`; every
//! failure is moved to `quarantine/` with a small reason marker. No socket or RPC endpoint is opened.

use kaspa_consensus_core::palw::da::{PALW_DA_MAX_OBJECT_BYTES, PALW_RECEIPT_DA_OBJECT_VERSION_V2, palw_receipt_da_commitment};
use kaspa_core::{
    info,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace, warn,
};
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use kaspa_p2p_flows::flow_context::FlowContext;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::{CString, OsStr},
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::{
    ffi::OsStrExt,
    fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
};

const PALW_DA_SPOOL: &str = "palw-da-spool";
const SPOOL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_JOBS_PER_TICK: usize = 16;
const MAX_INCOMING_DIRECTORY_ENTRIES_PER_TICK: usize = 64;
const MAX_PROCESSING_DIRECTORY_ENTRIES_PER_TICK: usize = 64;
const MAX_ARCHIVE_AUDITS_PER_TICK: usize = 4;
const MAX_ARCHIVE_DIRECTORY_ENTRIES_PER_TICK: usize = 32;
const MAX_METADATA_BYTES: usize = 4 * 1024;
const COMPLETE_SCHEMA: &str = "misaka.palw.da-spool-complete.v1";
const INVALID_NAME_HASH_CONTEXT: &[u8] = b"Misaka/PALW/da-spool-invalid-name/v1";

#[derive(Clone, Debug)]
pub struct PalwDaSpoolConfig {
    root: PathBuf,
    incoming: PathBuf,
    processing: PathBuf,
    archive: PathBuf,
    quarantine: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwDaSpoolEntryV1 {
    pub schema: String,
    pub batch_id: String,
    pub leaf_index: u32,
    pub object_root: String,
    pub object_len: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletionMarker {
    schema: String,
    batch_id: String,
    leaf_index: u32,
    object_root: String,
}

#[derive(Debug, Error)]
pub enum PalwDaSpoolError {
    #[error("PALW DA spool is supported only on Unix where owner/mode checks are available")]
    UnsupportedPlatform,
    #[error("PALW DA spool path must be absolute: {0}")]
    RelativePath(PathBuf),
    #[error("PALW DA spool filesystem error at {0}: {1}")]
    Io(PathBuf, String),
    #[error("PALW DA spool path is not a secure owner-only regular file/directory: {0}")]
    InsecurePath(PathBuf),
    #[error("PALW DA spool job name is invalid")]
    InvalidJobName,
    #[error("PALW DA spool metadata is malformed: {0}")]
    InvalidMetadata(String),
    #[error("PALW DA spool object is malformed: {0}")]
    InvalidObject(String),
    #[error("PALW DA selected-chain admission rejected the object: {0}")]
    Admission(String),
}

fn io_error(path: &Path, error: impl std::fmt::Display) -> PalwDaSpoolError {
    PalwDaSpoolError::Io(path.to_path_buf(), error.to_string())
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and does not retain pointers.
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn secure_metadata(path: &Path, directory: bool) -> Result<fs::Metadata, PalwDaSpoolError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(path, error))?;
    let expected_kind = if directory { metadata.file_type().is_dir() } else { metadata.file_type().is_file() };
    if !expected_kind
        || metadata.file_type().is_symlink()
        || metadata.uid() != current_uid()
        || metadata.mode() & 0o077 != 0
        || (!directory && metadata.nlink() != 1)
    {
        return Err(PalwDaSpoolError::InsecurePath(path.to_path_buf()));
    }
    Ok(metadata)
}

#[cfg(not(unix))]
fn secure_metadata(_path: &Path, _directory: bool) -> Result<fs::Metadata, PalwDaSpoolError> {
    Err(PalwDaSpoolError::UnsupportedPlatform)
}

#[cfg(unix)]
fn ensure_secure_dir(path: &Path) -> Result<(), PalwDaSpoolError> {
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700).recursive(false);
        builder.create(path).map_err(|error| io_error(path, error))?;
    }
    secure_metadata(path, true).map(drop)
}

#[cfg(not(unix))]
fn ensure_secure_dir(_path: &Path) -> Result<(), PalwDaSpoolError> {
    Err(PalwDaSpoolError::UnsupportedPlatform)
}

fn sync_dir(path: &Path) -> Result<(), PalwDaSpoolError> {
    fs::File::open(path).and_then(|directory| directory.sync_all()).map_err(|error| io_error(path, error))
}

#[cfg(unix)]
fn path_cstring(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("path contains NUL: {}", path.display())))
}

/// Atomically rename `source` only if `destination` has no directory entry. Unlike
/// `Path::exists` followed by `rename`, this also refuses dangling symlinks and has no overwrite
/// race. Unsupported Unix kernels/filesystems fail closed.
#[cfg(target_os = "linux")]
fn atomic_rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    let source = path_cstring(source)?;
    let destination = path_cstring(destination)?;
    // The syscall is invoked directly rather than through `libc::renameat2` because that
    // binding exists ONLY for `target_env = "gnu"` in our pinned libc, while this `cfg` (and
    // every release artifact) also covers `x86_64-unknown-linux-musl` — the musl release
    // build failed to compile with `E0425: cannot find function renameat2`. Bumping libc
    // would not fix it either: musl only grew a `renameat2` wrapper in 1.2.6, and this
    // repository's toolchain is older, so the compile error would merely become a link
    // error. `SYS_renameat2` and `RENAME_NOREPLACE` are defined for both envs, and going
    // through `syscall` preserves the exact atomic no-replace semantics promised above.
    // SAFETY: both C strings are live for the call and AT_FDCWD needs no directory fd ownership.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    // `syscall` reports failure as -1 (errno set); a kernel/filesystem without renameat2
    // yields ENOSYS/EINVAL here, which keeps the documented fail-closed behaviour.
    if result == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(target_os = "macos")]
fn atomic_rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    let source = path_cstring(source)?;
    let destination = path_cstring(destination)?;
    // SAFETY: both C strings are live for the call; RENAME_EXCL gives atomic no-replace semantics.
    let result = unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn atomic_rename_noreplace(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "atomic no-replace rename is unavailable on this platform"))
}

fn rename_new(source: &Path, destination: &Path) -> Result<(), PalwDaSpoolError> {
    let source_dir = source.parent().ok_or_else(|| PalwDaSpoolError::InsecurePath(source.to_path_buf()))?;
    let destination_dir = destination.parent().ok_or_else(|| PalwDaSpoolError::InsecurePath(destination.to_path_buf()))?;
    atomic_rename_noreplace(source, destination).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            PalwDaSpoolError::InvalidMetadata(format!("refusing to overwrite {}", destination.display()))
        } else {
            io_error(source, error)
        }
    })?;
    sync_dir(source_dir)?;
    if source_dir != destination_dir {
        sync_dir(destination_dir)?;
    }
    Ok(())
}

fn path_entry_exists(path: &Path) -> Result<bool, PalwDaSpoolError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(path, error)),
    }
}

#[cfg(unix)]
fn open_secure_bounded(path: &Path, cap: usize) -> Result<Vec<u8>, PalwDaSpoolError> {
    let before = secure_metadata(path, false)?;
    if before.len() > cap as u64 {
        return Err(PalwDaSpoolError::InvalidObject(format!("{} bytes exceeds {cap}", before.len())));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| io_error(path, error))?;
    let after = file.metadata().map_err(|error| io_error(path, error))?;
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(PalwDaSpoolError::InsecurePath(path.to_path_buf()));
    }
    let mut bytes = Vec::with_capacity(after.len() as usize);
    Read::by_ref(&mut file).take((cap + 1) as u64).read_to_end(&mut bytes).map_err(|error| io_error(path, error))?;
    if bytes.len() > cap {
        return Err(PalwDaSpoolError::InvalidObject(format!("stream exceeds {cap} bytes")));
    }
    Ok(bytes)
}

#[cfg(not(unix))]
fn open_secure_bounded(_path: &Path, _cap: usize) -> Result<Vec<u8>, PalwDaSpoolError> {
    Err(PalwDaSpoolError::UnsupportedPlatform)
}

fn parse_hash(value: &str, field: &str) -> Result<Hash64, PalwDaSpoolError> {
    let mut bytes = [0u8; 64];
    if value.len() != 128 || faster_hex::hex_decode(value.as_bytes(), &mut bytes).is_err() {
        return Err(PalwDaSpoolError::InvalidMetadata(format!("{field} must be 128 lowercase/uppercase hex characters")));
    }
    Ok(Hash64::from_bytes(bytes))
}

fn valid_job_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[derive(Default)]
struct DirectoryScanState {
    iterator: Option<fs::ReadDir>,
}

fn next_incoming_metadata_jobs(
    directory: &Path,
    state: &mut DirectoryScanState,
    entry_limit: usize,
    job_limit: usize,
) -> Result<(Vec<PathBuf>, usize), PalwDaSpoolError> {
    if entry_limit == 0 || job_limit == 0 {
        return Ok((Vec::new(), 0));
    }
    if state.iterator.is_none() {
        state.iterator = Some(fs::read_dir(directory).map_err(|error| io_error(directory, error))?);
    }
    let iterator = state.iterator.as_mut().expect("incoming iterator initialized");
    let mut jobs = BTreeSet::new();
    let mut consumed = 0;
    for _ in 0..entry_limit {
        let Some(entry) = iterator.next() else {
            state.iterator = None;
            break;
        };
        consumed += 1;
        let entry = entry.map_err(|error| io_error(directory, error))?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("json")) {
            continue;
        }
        jobs.insert(path);
        if jobs.len() >= job_limit {
            break;
        }
    }
    Ok((jobs.into_iter().collect(), consumed))
}

#[cfg(unix)]
fn invalid_name_token(path: &Path) -> String {
    let name = path.file_name().map(OsStrExt::as_bytes).unwrap_or_default();
    faster_hex::hex_string(blake2b_512_keyed(INVALID_NAME_HASH_CONTEXT, name).as_byte_slice())
}

fn quarantine_unclaimed_files(config: &PalwDaSpoolConfig, metadata: &Path, object: Option<&Path>, reason: &str) {
    let token = invalid_name_token(metadata);
    let metadata_destination = config.quarantine.join(format!("invalid-{token}.json"));
    if let Err(error) = rename_new(metadata, &metadata_destination) {
        warn!("[{PALW_DA_SPOOL}] cannot quarantine malformed metadata {}: {error}", metadata.display());
    }
    if let Some(object) = object {
        let object_destination = config.quarantine.join(format!("invalid-{token}.palwda"));
        if let Err(error) = rename_new(object, &object_destination) {
            warn!("[{PALW_DA_SPOOL}] cannot quarantine malformed paired object {}: {error}", object.display());
        }
    }
    let marker = config.quarantine.join(format!("invalid-{token}.error.txt"));
    if write_owner_only_new(&marker, reason.as_bytes()).is_ok() {
        let _ = sync_dir(&config.quarantine);
    }
}

#[cfg(not(unix))]
fn invalid_name_token(path: &Path) -> String {
    faster_hex::hex_string(blake2b_512_keyed(INVALID_NAME_HASH_CONTEXT, path.as_os_str().to_string_lossy().as_bytes()).as_byte_slice())
}

/// Advance one persistent, bounded directory window. Keeping the open iterator across ticks avoids
/// rescanning a growing archive from the beginning; reaching EOF resets it so every retained entry is
/// audited again on the next rotation. Each valid stem has at most three recognized filenames, so
/// finite archives cannot starve a later stem.
fn next_spool_stems(
    directory: &Path,
    state: &mut DirectoryScanState,
    entry_limit: usize,
    stem_limit: usize,
) -> Result<(Vec<String>, usize), PalwDaSpoolError> {
    if entry_limit == 0 || stem_limit == 0 {
        return Ok((Vec::new(), 0));
    }
    if state.iterator.is_none() {
        state.iterator = Some(fs::read_dir(directory).map_err(|error| io_error(directory, error))?);
    }
    let iterator = state.iterator.as_mut().expect("archive iterator initialized");
    let mut stems = BTreeSet::new();
    let mut consumed = 0;
    for _ in 0..entry_limit {
        let Some(entry) = iterator.next() else {
            state.iterator = None;
            break;
        };
        consumed += 1;
        let entry = entry.map_err(|error| io_error(directory, error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let stem = name.strip_suffix(".complete.json").or_else(|| name.strip_suffix(".palwda")).or_else(|| name.strip_suffix(".json"));
        let Some(stem) = stem.filter(|stem| valid_job_name(stem)) else { continue };
        stems.insert(stem.to_string());
        if stems.len() >= stem_limit {
            break;
        }
    }
    Ok((stems.into_iter().collect(), consumed))
}

fn validate_pair(metadata_path: &Path, object_path: &Path) -> Result<(PalwDaSpoolEntryV1, Hash64, Hash64, Vec<u8>), PalwDaSpoolError> {
    let metadata_bytes = open_secure_bounded(metadata_path, MAX_METADATA_BYTES)?;
    let entry: PalwDaSpoolEntryV1 =
        serde_json::from_slice(&metadata_bytes).map_err(|error| PalwDaSpoolError::InvalidMetadata(error.to_string()))?;
    if entry.schema != "misaka.palw.da-spool-entry.v1" {
        return Err(PalwDaSpoolError::InvalidMetadata("unsupported schema".into()));
    }
    let batch_id = parse_hash(&entry.batch_id, "batch_id")?;
    let expected_root = parse_hash(&entry.object_root, "object_root")?;
    let object = open_secure_bounded(object_path, PALW_DA_MAX_OBJECT_BYTES)?;
    let version = object
        .get(..2)
        .and_then(|prefix| prefix.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| PalwDaSpoolError::InvalidObject("missing version".into()))?;
    if version != PALW_RECEIPT_DA_OBJECT_VERSION_V2 || object.len() != entry.object_len as usize {
        return Err(PalwDaSpoolError::InvalidObject("metadata/version/length mismatch".into()));
    }
    let commitment =
        palw_receipt_da_commitment(version, &object).map_err(|error| PalwDaSpoolError::InvalidObject(error.to_string()))?;
    if commitment.root != expected_root {
        return Err(PalwDaSpoolError::InvalidObject("content root mismatch".into()));
    }
    Ok((entry, batch_id, expected_root, object))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessingPairState {
    Ready,
    Incomplete,
}

fn repair_processing_pair(config: &PalwDaSpoolConfig, stem: &str) -> Result<ProcessingPairState, PalwDaSpoolError> {
    let processing_metadata = config.processing.join(format!("{stem}.json"));
    let processing_object = config.processing.join(format!("{stem}.palwda"));
    let incoming_metadata = config.incoming.join(format!("{stem}.json"));
    let incoming_object = config.incoming.join(format!("{stem}.palwda"));

    if !path_entry_exists(&processing_object)? && path_entry_exists(&incoming_object)? {
        secure_metadata(&incoming_object, false)?;
        rename_new(&incoming_object, &processing_object)?;
    }
    if !path_entry_exists(&processing_metadata)? && path_entry_exists(&incoming_metadata)? {
        secure_metadata(&incoming_metadata, false)?;
        rename_new(&incoming_metadata, &processing_metadata)?;
    }
    Ok(if path_entry_exists(&processing_metadata)? && path_entry_exists(&processing_object)? {
        ProcessingPairState::Ready
    } else {
        ProcessingPairState::Incomplete
    })
}

fn quarantine_one_fs(config: &PalwDaSpoolConfig, source: &Path, destination_name: &str) -> Result<(), PalwDaSpoolError> {
    rename_new(source, &config.quarantine.join(destination_name))
}

fn rollback_partial_archive(config: &PalwDaSpoolConfig, stem: &str) -> Result<(), PalwDaSpoolError> {
    let archived_metadata = config.archive.join(format!("{stem}.json"));
    let archived_object = config.archive.join(format!("{stem}.palwda"));
    for (source, destination, label) in [
        (&archived_object, config.processing.join(format!("{stem}.palwda")), "object"),
        (&archived_metadata, config.processing.join(format!("{stem}.json")), "metadata"),
    ] {
        if path_entry_exists(source)? {
            if path_entry_exists(&destination)? {
                quarantine_one_fs(config, source, &format!("{stem}.archive-partial-{label}"))?;
            } else {
                rename_new(source, &destination)?;
            }
        }
    }
    Ok(())
}

fn validate_complete_archive(metadata: &Path, object: &Path, marker: &Path) -> Result<(), PalwDaSpoolError> {
    let (entry, _, _, _) = validate_pair(metadata, object)?;
    let marker_bytes = open_secure_bounded(marker, MAX_METADATA_BYTES)?;
    let completion: CompletionMarker =
        serde_json::from_slice(&marker_bytes).map_err(|error| PalwDaSpoolError::InvalidMetadata(error.to_string()))?;
    if completion.schema != COMPLETE_SCHEMA
        || completion.batch_id != entry.batch_id
        || completion.leaf_index != entry.leaf_index
        || completion.object_root != entry.object_root
    {
        return Err(PalwDaSpoolError::InvalidMetadata("completion marker mismatch".into()));
    }
    Ok(())
}

impl PalwDaSpoolConfig {
    pub fn prepare(root: impl Into<PathBuf>) -> Result<Self, PalwDaSpoolError> {
        let root = root.into();
        if !root.is_absolute() {
            return Err(PalwDaSpoolError::RelativePath(root));
        }
        ensure_secure_dir(&root)?;
        let incoming = root.join("incoming");
        let processing = root.join("processing");
        let archive = root.join("archive");
        let quarantine = root.join("quarantine");
        for path in [&incoming, &processing, &archive, &quarantine] {
            ensure_secure_dir(path)?;
        }
        sync_dir(&root)?;
        Ok(Self { root, incoming, processing, archive, quarantine })
    }
}

pub struct PalwDaSpoolService {
    config: PalwDaSpoolConfig,
    tick_service: Arc<TickService>,
    flow_context: Arc<FlowContext>,
    incoming_scan: Mutex<DirectoryScanState>,
    processing_scan: Mutex<DirectoryScanState>,
    archive_audit: Mutex<DirectoryScanState>,
}

impl PalwDaSpoolService {
    pub fn new(config: PalwDaSpoolConfig, tick_service: Arc<TickService>, flow_context: Arc<FlowContext>) -> Self {
        Self {
            config,
            tick_service,
            flow_context,
            incoming_scan: Mutex::new(DirectoryScanState::default()),
            processing_scan: Mutex::new(DirectoryScanState::default()),
            archive_audit: Mutex::new(DirectoryScanState::default()),
        }
    }

    async fn worker(self: &Arc<Self>) {
        info!("[{PALW_DA_SPOOL}] enabled at {} (local filesystem only)", self.config.root.display());
        loop {
            if let Err(error) = self.process_tick().await {
                warn!("[{PALW_DA_SPOOL}] scan failed: {error}");
            }
            if let TickReason::Shutdown = self.tick_service.tick(SPOOL_INTERVAL).await {
                break;
            }
        }
        info!("[{PALW_DA_SPOOL}] stopped");
    }

    async fn process_tick(&self) -> Result<(), PalwDaSpoolError> {
        // Re-check directory ownership/mode each tick: changing permissions after startup must fail
        // closed instead of silently weakening the local authentication boundary.
        for path in [&self.config.root, &self.config.incoming, &self.config.processing, &self.config.archive, &self.config.quarantine]
        {
            ensure_secure_dir(path)?;
        }
        self.reconcile_archive()?;
        self.reconcile_processing().await?;

        let incoming_jobs = {
            let mut scan = self
                .incoming_scan
                .lock()
                .map_err(|_| PalwDaSpoolError::InvalidMetadata("incoming scan cursor lock is poisoned".into()))?;
            next_incoming_metadata_jobs(&self.config.incoming, &mut scan, MAX_INCOMING_DIRECTORY_ENTRIES_PER_TICK, MAX_JOBS_PER_TICK)?
                .0
        };
        for metadata_path in incoming_jobs {
            let paired_object = metadata_path.with_extension("palwda");
            let Some(stem) = metadata_path.file_stem().and_then(|value| value.to_str()) else {
                self.quarantine_unclaimed(
                    &metadata_path,
                    path_entry_exists(&paired_object)?.then_some(paired_object.as_path()),
                    "non-UTF8 job name",
                );
                continue;
            };
            if !valid_job_name(stem) {
                self.quarantine_unclaimed(
                    &metadata_path,
                    path_entry_exists(&paired_object)?.then_some(paired_object.as_path()),
                    "invalid job name",
                );
                continue;
            }
            if let Err(error) = self.claim_incoming(stem) {
                warn!("[{PALW_DA_SPOOL}] quarantined {stem}: {error}");
                self.quarantine_claimed(stem, &error.to_string());
                continue;
            }
            if let Err(error) = self.process_claimed(stem).await {
                warn!("[{PALW_DA_SPOOL}] quarantined {stem}: {error}");
                self.quarantine_claimed(stem, &error.to_string());
            }
        }
        Ok(())
    }

    fn claim_incoming(&self, stem: &str) -> Result<(), PalwDaSpoolError> {
        let incoming_metadata = self.config.incoming.join(format!("{stem}.json"));
        let incoming_object = self.config.incoming.join(format!("{stem}.palwda"));
        let processing_metadata = self.config.processing.join(format!("{stem}.json"));
        let processing_object = self.config.processing.join(format!("{stem}.palwda"));
        if path_entry_exists(&processing_metadata)? || path_entry_exists(&processing_object)? {
            return Err(PalwDaSpoolError::InvalidMetadata("processing destination already exists".into()));
        }
        secure_metadata(&incoming_metadata, false)?;
        secure_metadata(&incoming_object, false)?;
        // Producer metadata is the ready marker. Claim the object first so every crash prefix has a
        // deterministic recovery: processing object + incoming metadata is completed next tick.
        rename_new(&incoming_object, &processing_object)?;
        rename_new(&incoming_metadata, &processing_metadata)
    }

    async fn reconcile_processing(&self) -> Result<(), PalwDaSpoolError> {
        let stems = {
            let mut scan = self
                .processing_scan
                .lock()
                .map_err(|_| PalwDaSpoolError::InvalidMetadata("processing scan cursor lock is poisoned".into()))?;
            next_spool_stems(&self.config.processing, &mut scan, MAX_PROCESSING_DIRECTORY_ENTRIES_PER_TICK, MAX_JOBS_PER_TICK)?.0
        };
        for stem in stems {
            if repair_processing_pair(&self.config, &stem)? == ProcessingPairState::Ready {
                // Admission and durable insertion are content-addressed/idempotent. Always rerun them
                // after a crash rather than trusting that a previous attempt reached the database.
                if let Err(error) = self.process_claimed(&stem).await {
                    warn!("[{PALW_DA_SPOOL}] quarantined recovered job {stem}: {error}");
                    self.quarantine_claimed(&stem, &error.to_string());
                }
            } else {
                self.quarantine_claimed(&stem, "incomplete processing pair after deterministic reconciliation");
            }
        }
        Ok(())
    }

    async fn process_claimed(&self, stem: &str) -> Result<(), PalwDaSpoolError> {
        let processing_metadata = self.config.processing.join(format!("{stem}.json"));
        let processing_object = self.config.processing.join(format!("{stem}.palwda"));
        let (entry, batch_id, expected_root, object) = validate_pair(&processing_metadata, &processing_object)?;

        let consensus = self.flow_context.consensus().unguarded_session();
        let admitted_root = self
            .flow_context
            .cache_palw_da_object(&consensus, batch_id, entry.leaf_index, Arc::new(object))
            .await
            .map_err(|error| PalwDaSpoolError::Admission(error.to_string()))?;
        if admitted_root != expected_root {
            return Err(PalwDaSpoolError::Admission("admission returned another root".into()));
        }
        self.archive(stem, &entry)?;
        info!("[{PALW_DA_SPOOL}] admitted and archived Object-v2 {admitted_root}");
        Ok(())
    }

    fn reconcile_archive(&self) -> Result<(), PalwDaSpoolError> {
        let stems = {
            let mut audit = self
                .archive_audit
                .lock()
                .map_err(|_| PalwDaSpoolError::InvalidMetadata("archive audit cursor lock is poisoned".into()))?;
            next_spool_stems(&self.config.archive, &mut audit, MAX_ARCHIVE_DIRECTORY_ENTRIES_PER_TICK, MAX_ARCHIVE_AUDITS_PER_TICK)?.0
        };
        for stem in stems {
            let archived_metadata = self.config.archive.join(format!("{stem}.json"));
            let archived_object = self.config.archive.join(format!("{stem}.palwda"));
            let marker = self.config.archive.join(format!("{stem}.complete.json"));
            if path_entry_exists(&marker)? {
                let terminal_valid = path_entry_exists(&archived_metadata)?
                    && path_entry_exists(&archived_object)?
                    && validate_complete_archive(&archived_metadata, &archived_object, &marker).is_ok();
                if !terminal_valid {
                    self.quarantine_archive(&stem, "complete marker has missing/mismatched archive artifacts");
                }
            } else {
                // No terminal marker means the archive transition was interrupted. Roll each
                // available artifact back to processing and rerun full admission. Never overwrite a
                // processing file: if both copies exist, quarantine only the partial archive copy.
                rollback_partial_archive(&self.config, &stem)?;
            }
        }
        Ok(())
    }

    fn archive(&self, stem: &str, entry: &PalwDaSpoolEntryV1) -> Result<(), PalwDaSpoolError> {
        let source_metadata = self.config.processing.join(format!("{stem}.json"));
        let source_object = self.config.processing.join(format!("{stem}.palwda"));
        let archived_metadata = self.config.archive.join(format!("{stem}.json"));
        let archived_object = self.config.archive.join(format!("{stem}.palwda"));
        let marker = self.config.archive.join(format!("{stem}.complete.json"));
        if path_entry_exists(&archived_metadata)? || path_entry_exists(&archived_object)? || path_entry_exists(&marker)? {
            return Err(PalwDaSpoolError::InvalidMetadata("archive destination already exists".into()));
        }
        rename_new(&source_object, &archived_object)?;
        rename_new(&source_metadata, &archived_metadata)?;
        let completion = CompletionMarker {
            schema: COMPLETE_SCHEMA.to_string(),
            batch_id: entry.batch_id.clone(),
            leaf_index: entry.leaf_index,
            object_root: entry.object_root.clone(),
        };
        write_owner_only_new(&marker, &serde_json::to_vec_pretty(&completion).expect("completion marker is serializable"))?;
        sync_dir(&self.config.archive)
    }

    fn quarantine_claimed(&self, stem: &str, reason: &str) {
        for (directory, label) in [(&self.config.processing, ""), (&self.config.incoming, "incoming-")] {
            for extension in ["json", "palwda"] {
                let source = directory.join(format!("{stem}.{extension}"));
                if path_entry_exists(&source).unwrap_or(false) {
                    self.quarantine_one(&source, &format!("{stem}.{label}{extension}"));
                }
            }
        }
        let marker = self.config.quarantine.join(format!("{stem}.error.txt"));
        let bounded = &reason.as_bytes()[..reason.len().min(1_024)];
        if write_owner_only_new(&marker, bounded).is_ok() {
            let _ = sync_dir(&self.config.quarantine);
        }
    }

    fn quarantine_archive(&self, stem: &str, reason: &str) {
        for suffix in ["json", "palwda", "complete.json"] {
            let source = self.config.archive.join(format!("{stem}.{suffix}"));
            if path_entry_exists(&source).unwrap_or(false) {
                self.quarantine_one(&source, &format!("{stem}.archive-{suffix}"));
            }
        }
        let marker = self.config.quarantine.join(format!("{stem}.archive-error.txt"));
        if write_owner_only_new(&marker, reason.as_bytes()).is_ok() {
            let _ = sync_dir(&self.config.quarantine);
        }
    }

    fn quarantine_one(&self, source: &Path, destination_name: &str) {
        let destination = self.config.quarantine.join(destination_name);
        if let Err(error) = rename_new(source, &destination) {
            warn!("[{PALW_DA_SPOOL}] cannot quarantine {}: {error}", source.display());
        }
    }

    fn quarantine_unclaimed(&self, metadata: &Path, object: Option<&Path>, reason: &str) {
        quarantine_unclaimed_files(&self.config, metadata, object, reason)
    }
}

#[cfg(unix)]
fn write_owner_only_new(path: &Path, bytes: &[u8]) -> Result<(), PalwDaSpoolError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| io_error(path, error))?;
    file.write_all(bytes).and_then(|()| file.sync_all()).map_err(|error| io_error(path, error))
}

#[cfg(not(unix))]
fn write_owner_only_new(_path: &Path, _bytes: &[u8]) -> Result<(), PalwDaSpoolError> {
    Err(PalwDaSpoolError::UnsupportedPlatform)
}

impl AsyncService for PalwDaSpoolService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_DA_SPOOL
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {PALW_DA_SPOOL}");
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async { Ok(()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
        sync::{Arc as StdArc, Barrier},
        thread,
    };

    #[cfg(unix)]
    fn fixture(config: &PalwDaSpoolConfig, directory: &Path, stem: &str) -> PalwDaSpoolEntryV1 {
        let mut object = vec![0x42; 20_000];
        object[..2].copy_from_slice(&PALW_RECEIPT_DA_OBJECT_VERSION_V2.to_le_bytes());
        let commitment = palw_receipt_da_commitment(PALW_RECEIPT_DA_OBJECT_VERSION_V2, &object).unwrap();
        let entry = PalwDaSpoolEntryV1 {
            schema: "misaka.palw.da-spool-entry.v1".into(),
            batch_id: "02".repeat(64),
            leaf_index: 7,
            object_root: faster_hex::hex_string(commitment.root.as_byte_slice()),
            object_len: commitment.object_len,
        };
        write_owner_only_new(&directory.join(format!("{stem}.palwda")), &object).unwrap();
        write_owner_only_new(&directory.join(format!("{stem}.json")), &serde_json::to_vec(&entry).unwrap()).unwrap();
        sync_dir(directory).unwrap();
        // Touch the config so callers cannot accidentally construct a fixture outside its tree.
        assert!(directory.starts_with(&config.root));
        entry
    }

    #[cfg(unix)]
    #[test]
    fn spool_prepare_is_owner_only_and_rejects_symlink_root() {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("spool");
        let config = PalwDaSpoolConfig::prepare(&root).unwrap();
        for path in [&config.root, &config.incoming, &config.processing, &config.archive, &config.quarantine] {
            assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o700);
        }
        let link = parent.path().join("link");
        std::os::unix::fs::symlink(&root, &link).unwrap();
        assert!(matches!(PalwDaSpoolConfig::prepare(link), Err(PalwDaSpoolError::InsecurePath(_))));
    }

    #[cfg(unix)]
    #[test]
    fn secure_reader_rejects_symlink_hardlink_world_readable_and_oversize() {
        let parent = TempDir::new().unwrap();
        let file = parent.path().join("object");
        write_owner_only_new(&file, &[2, 0, 1]).unwrap();
        assert_eq!(open_secure_bounded(&file, 3).unwrap(), [2, 0, 1]);

        let link = parent.path().join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(matches!(open_secure_bounded(&link, 3), Err(PalwDaSpoolError::InsecurePath(_))));

        let hard = parent.path().join("hard");
        fs::hard_link(&file, &hard).unwrap();
        assert!(matches!(open_secure_bounded(&file, 3), Err(PalwDaSpoolError::InsecurePath(_))));
        fs::remove_file(hard).unwrap();

        let mut permissions = fs::metadata(&file).unwrap().permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&file, permissions).unwrap();
        assert!(matches!(open_secure_bounded(&file, 3), Err(PalwDaSpoolError::InsecurePath(_))));
        permissions = fs::metadata(&file).unwrap().permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&file, permissions).unwrap();
        assert!(matches!(open_secure_bounded(&file, 2), Err(PalwDaSpoolError::InvalidObject(_))));
    }

    #[cfg(unix)]
    #[test]
    fn atomic_noreplace_refuses_regular_dangling_symlink_and_has_one_race_winner() {
        let directory = TempDir::new().unwrap();

        let regular_source = directory.path().join("regular-source");
        let regular_destination = directory.path().join("regular-destination");
        write_owner_only_new(&regular_source, b"new").unwrap();
        write_owner_only_new(&regular_destination, b"old").unwrap();
        assert!(rename_new(&regular_source, &regular_destination).is_err());
        assert_eq!(fs::read(&regular_source).unwrap(), b"new");
        assert_eq!(fs::read(&regular_destination).unwrap(), b"old");

        let symlink_source = directory.path().join("symlink-source");
        let symlink_destination = directory.path().join("dangling-destination");
        let missing_target = directory.path().join("missing-target");
        write_owner_only_new(&symlink_source, b"new").unwrap();
        std::os::unix::fs::symlink(&missing_target, &symlink_destination).unwrap();
        assert!(!symlink_destination.exists(), "the test destination must be a dangling symlink");
        assert!(rename_new(&symlink_source, &symlink_destination).is_err());
        assert_eq!(fs::read_link(&symlink_destination).unwrap(), missing_target);
        assert_eq!(fs::read(&symlink_source).unwrap(), b"new");

        let first = directory.path().join("race-first");
        let second = directory.path().join("race-second");
        let destination = directory.path().join("race-destination");
        write_owner_only_new(&first, b"first").unwrap();
        write_owner_only_new(&second, b"second").unwrap();
        let barrier = StdArc::new(Barrier::new(3));
        let racers = [first.clone(), second.clone()]
            .into_iter()
            .map(|source| {
                let barrier = StdArc::clone(&barrier);
                let destination = destination.clone();
                thread::spawn(move || {
                    barrier.wait();
                    rename_new(&source, &destination)
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = racers.into_iter().map(|racer| racer.join().unwrap()).collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let winner = fs::read(&destination).unwrap();
        assert!(winner == b"first" || winner == b"second");
        assert_eq!(path_entry_exists(&first).unwrap() as u8 + path_entry_exists(&second).unwrap() as u8, 1);
    }

    #[cfg(unix)]
    #[test]
    fn malformed_and_non_utf8_metadata_are_bounded_and_quarantine_their_pairs() {
        let parent = TempDir::new().unwrap();
        let config = PalwDaSpoolConfig::prepare(parent.path().join("spool")).unwrap();

        let utf8_metadata = config.incoming.join("with.dot.json");
        let utf8_object = config.incoming.join("with.dot.palwda");
        write_owner_only_new(&utf8_metadata, b"{}").unwrap();
        write_owner_only_new(&utf8_object, b"object").unwrap();

        let non_utf8_stem = OsString::from_vec(vec![b'n', b'o', b'n', 0xff, b'u', b't', b'f', b'8']);
        let mut synthetic_non_utf8_metadata = config.incoming.join(&non_utf8_stem);
        synthetic_non_utf8_metadata.set_extension("json");
        assert!(synthetic_non_utf8_metadata.file_stem().and_then(|stem| stem.to_str()).is_none());

        // Linux filesystems permit arbitrary non-NUL filename bytes. APFS rejects this entry at the
        // filesystem boundary with EILSEQ, so macOS exercises the identical branch above without
        // pretending such a directory entry can exist there.
        #[cfg(target_os = "linux")]
        let mut non_utf8_metadata = config.incoming.join(&non_utf8_stem);
        #[cfg(target_os = "linux")]
        non_utf8_metadata.set_extension("json");
        #[cfg(target_os = "linux")]
        let mut non_utf8_object = config.incoming.join(&non_utf8_stem);
        #[cfg(target_os = "linux")]
        non_utf8_object.set_extension("palwda");
        #[cfg(target_os = "linux")]
        write_owner_only_new(&non_utf8_metadata, b"{}").unwrap();
        #[cfg(target_os = "linux")]
        write_owner_only_new(&non_utf8_object, b"object").unwrap();

        let mut incoming_scan = DirectoryScanState::default();
        let (jobs, consumed) = next_incoming_metadata_jobs(&config.incoming, &mut incoming_scan, 64, MAX_JOBS_PER_TICK).unwrap();
        assert!(consumed <= 64);
        assert!(jobs.contains(&utf8_metadata));
        #[cfg(target_os = "linux")]
        {
            assert_eq!(jobs.len(), 2);
            assert!(jobs.contains(&non_utf8_metadata));
        }
        #[cfg(not(target_os = "linux"))]
        assert_eq!(jobs.len(), 1);

        #[cfg(target_os = "linux")]
        let malformed_pairs = vec![(&utf8_metadata, &utf8_object), (&non_utf8_metadata, &non_utf8_object)];
        #[cfg(not(target_os = "linux"))]
        let malformed_pairs = vec![(&utf8_metadata, &utf8_object)];
        for (metadata, object) in malformed_pairs {
            let token = invalid_name_token(metadata);
            quarantine_unclaimed_files(&config, metadata, Some(object), "invalid job name");
            assert!(!path_entry_exists(metadata).unwrap());
            assert!(!path_entry_exists(object).unwrap());
            assert!(config.quarantine.join(format!("invalid-{token}.json")).is_file());
            assert!(config.quarantine.join(format!("invalid-{token}.palwda")).is_file());
            assert!(config.quarantine.join(format!("invalid-{token}.error.txt")).is_file());
        }

        for index in 0..MAX_JOBS_PER_TICK + 5 {
            write_owner_only_new(&config.incoming.join(format!("bad.name-{index:03}.json")), b"{}").unwrap();
        }
        let mut bounded_scan = DirectoryScanState::default();
        let (bounded, consumed) = next_incoming_metadata_jobs(&config.incoming, &mut bounded_scan, 64, MAX_JOBS_PER_TICK).unwrap();
        assert_eq!(bounded.len(), MAX_JOBS_PER_TICK);
        assert!(consumed <= 64);
    }

    #[cfg(unix)]
    #[test]
    fn incoming_cursor_bounds_irrelevant_backlog_and_eventually_reaches_every_metadata_job() {
        let parent = TempDir::new().unwrap();
        let config = PalwDaSpoolConfig::prepare(parent.path().join("spool")).unwrap();
        for index in 0..100 {
            write_owner_only_new(&config.incoming.join(format!("irrelevant-{index:03}.palwda.tmp")), b"x").unwrap();
        }
        let expected = (0..20).map(|index| config.incoming.join(format!("job-{index:03}.json"))).collect::<BTreeSet<_>>();
        for path in &expected {
            write_owner_only_new(path, b"{}").unwrap();
        }

        let mut scan = DirectoryScanState::default();
        let mut seen = BTreeSet::new();
        for _ in 0..64 {
            let (jobs, consumed) = next_incoming_metadata_jobs(&config.incoming, &mut scan, 7, 3).unwrap();
            assert!(consumed <= 7, "irrelevant files must consume the same bounded entry budget");
            assert!(jobs.len() <= 3);
            seen.extend(jobs);
            if seen == expected {
                break;
            }
        }
        assert_eq!(seen, expected, "persistent cursor must eventually pass irrelevant backlog and reach every finite job");
    }

    #[cfg(unix)]
    #[test]
    fn archive_audit_rotates_with_bounded_windows_without_starvation() {
        let parent = TempDir::new().unwrap();
        let config = PalwDaSpoolConfig::prepare(parent.path().join("spool")).unwrap();
        let expected = (0..10).map(|index| format!("archive-{index:02}")).collect::<BTreeSet<_>>();
        for stem in &expected {
            for suffix in ["json", "palwda", "complete.json"] {
                write_owner_only_new(&config.archive.join(format!("{stem}.{suffix}")), b"x").unwrap();
            }
        }

        let mut state = DirectoryScanState::default();
        let mut seen = BTreeSet::new();
        for _ in 0..20 {
            let (window, consumed) = next_spool_stems(&config.archive, &mut state, 5, 2).unwrap();
            assert!(consumed <= 5);
            assert!(window.len() <= 2);
            seen.extend(window);
            if seen == expected {
                break;
            }
        }
        assert_eq!(seen, expected, "persistent directory rotation must eventually reach every retained archive stem");
    }

    #[cfg(unix)]
    #[test]
    fn processing_cursor_bounds_irrelevant_backlog_and_reaches_every_finite_stem() {
        let parent = TempDir::new().unwrap();
        let config = PalwDaSpoolConfig::prepare(parent.path().join("spool")).unwrap();
        for index in 0..100 {
            write_owner_only_new(&config.processing.join(format!(".irrelevant-{index:03}.tmp")), b"x").unwrap();
        }
        let expected = (0..20).map(|index| format!("processing-{index:03}")).collect::<BTreeSet<_>>();
        for stem in &expected {
            write_owner_only_new(&config.processing.join(format!("{stem}.json")), b"{}").unwrap();
        }

        let mut scan = DirectoryScanState::default();
        let mut seen = BTreeSet::new();
        for _ in 0..64 {
            let (stems, consumed) = next_spool_stems(&config.processing, &mut scan, 7, 3).unwrap();
            assert!(consumed <= 7);
            assert!(stems.len() <= 3);
            seen.extend(stems);
            if seen == expected {
                break;
            }
        }
        assert_eq!(seen, expected, "processing cursor must not starve finite jobs behind irrelevant entries");
    }

    #[test]
    fn job_names_and_metadata_are_strict() {
        assert!(valid_job_name("object_01-root"));
        assert!(!valid_job_name("../escape"));
        assert!(!valid_job_name("with.dot"));
        let unknown = br#"{"schema":"misaka.palw.da-spool-entry.v1","batch_id":"00","leaf_index":0,"object_root":"00","object_len":1,"extra":true}"#;
        assert!(serde_json::from_slice::<PalwDaSpoolEntryV1>(unknown).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_object_first_claim_resumes_without_duplicate_or_overwrite() {
        let parent = TempDir::new().unwrap();
        let config = PalwDaSpoolConfig::prepare(parent.path().join("spool")).unwrap();
        let stem = "claim-object-first";
        fixture(&config, &config.incoming, stem);
        rename_new(&config.incoming.join(format!("{stem}.palwda")), &config.processing.join(format!("{stem}.palwda"))).unwrap();

        assert_eq!(repair_processing_pair(&config, stem).unwrap(), ProcessingPairState::Ready);
        assert!(config.processing.join(format!("{stem}.palwda")).is_file());
        assert!(config.processing.join(format!("{stem}.json")).is_file());
        assert!(!config.incoming.join(format!("{stem}.json")).exists());
        assert_eq!(repair_processing_pair(&config, stem).unwrap(), ProcessingPairState::Ready, "restart is idempotent");
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_archive_rolls_back_for_full_readmission_and_conflict_never_overwrites() {
        let parent = TempDir::new().unwrap();
        let config = PalwDaSpoolConfig::prepare(parent.path().join("spool")).unwrap();
        let stem = "archive-partial";
        fixture(&config, &config.archive, stem);

        rollback_partial_archive(&config, stem).unwrap();
        assert!(config.processing.join(format!("{stem}.palwda")).is_file());
        assert!(config.processing.join(format!("{stem}.json")).is_file());
        assert!(!config.archive.join(format!("{stem}.palwda")).exists());
        assert!(!config.archive.join(format!("{stem}.json")).exists());

        let conflict = "archive-conflict";
        write_owner_only_new(&config.processing.join(format!("{conflict}.palwda")), b"processing-wins").unwrap();
        write_owner_only_new(&config.archive.join(format!("{conflict}.palwda")), b"archive-copy").unwrap();
        rollback_partial_archive(&config, conflict).unwrap();
        assert_eq!(fs::read(config.processing.join(format!("{conflict}.palwda"))).unwrap(), b"processing-wins");
        assert_eq!(fs::read(config.quarantine.join(format!("{conflict}.archive-partial-object"))).unwrap(), b"archive-copy");
    }

    #[cfg(unix)]
    #[test]
    fn complete_marker_is_terminal_only_with_matching_pair() {
        let parent = TempDir::new().unwrap();
        let config = PalwDaSpoolConfig::prepare(parent.path().join("spool")).unwrap();
        let stem = "archive-complete";
        let entry = fixture(&config, &config.archive, stem);
        let marker = config.archive.join(format!("{stem}.complete.json"));
        write_owner_only_new(
            &marker,
            &serde_json::to_vec(&CompletionMarker {
                schema: COMPLETE_SCHEMA.into(),
                batch_id: entry.batch_id.clone(),
                leaf_index: entry.leaf_index,
                object_root: entry.object_root.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        sync_dir(&config.archive).unwrap();
        assert!(
            validate_complete_archive(
                &config.archive.join(format!("{stem}.json")),
                &config.archive.join(format!("{stem}.palwda")),
                &marker
            )
            .is_ok()
        );

        fs::remove_file(config.archive.join(format!("{stem}.palwda"))).unwrap();
        assert!(
            validate_complete_archive(
                &config.archive.join(format!("{stem}.json")),
                &config.archive.join(format!("{stem}.palwda")),
                &marker
            )
            .is_err()
        );
    }
}
